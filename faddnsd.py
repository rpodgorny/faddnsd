#!/usr/bin/python3

'''
freakin' awesome dynamic dns server

Usage:
  faddnsd [options] <zone> <zone_fn> [<serial_fn>]

Arguments:
  <zone>       Name of the DNS zone.
  <zone_fn>    Filename of zone file.
  <serial_fn>  Filename of zone file which contains serial.

Options:
  -p <port>, --port=<port>  Port number.
  --debug                   Enable debug output.
'''

from faddns.version import __version__

import sys
import os
import cherrypy
import datetime
import logging
import docopt
import json
import time
import subprocess
import re
import ipaddress
from cherrypy.process.plugins import Monitor


# TODO: get rid of this global shit
class G:
	pass
g = G()

# TODO: global shit
recs = {}
datetimes = {}
ts = {}
changed = set()
unpaired = set()
do_pair = set()


def dt_format(dt):
	return dt.strftime('%Y-%m-%d %H:%M:%S')


class FADDNSServer(object):
	@cherrypy.expose
	def index(self, version=None, host=None, *args, **kwargs):
		if not host:
			logging.info('no host specified, ignoring')
			return '''
<html>
<body>
<p>no host specified</p>
<p><a href="listhosts">listhosts</a></p>
<p><a href="dump">dump</a></p>
</body>
</html>
'''
		rec = {}
		rec['hostname'] = host
		rec['version'] = version
		rec['remote_addr'] = cherrypy.request.remote.ip
		for af in 'ether', 'inet', 'inet6':
			if not af in kwargs:
				continue
			if isinstance(kwargs[af], str):
				rec[af] = set([kwargs[af], ])
			else:
				rec[af] = set(kwargs[af])
		if rec != recs.get(host):
			recs[host] = rec
			changed.add(host)
		datetimes[host] = datetime.datetime.now()
		ts[host] = time.time()
		return 'OK'

	@cherrypy.expose
	def dump(self):
		for host, rec in recs.items():
			r = rec
			r['datetime'] = dt_format(datetimes[host])
			r['t'] = ts[host]
			# sets are not serializable to json -> convert them to lists
			for k, v in rec.items():
				if isinstance(v, set):
					r[k] = list(v)
			yield json.dumps(r) + '\n'
		return '\n'

	# TODO: i added this for vlada to simplify his life (as he's unable to parse jsons format above)
	# TODO: basically just a cut-n-paste of the code above - unite!
	@cherrypy.expose
	def dump2(self):
		ret = []
		for host, rec in recs.items():
			r = rec
			r['datetime'] = dt_format(datetimes[host])
			r['t'] = ts[host]
			# sets are not serializable to json -> convert them to lists
			for k, v in rec.items():
				if isinstance(v, set):
					r[k] = list(v)
			ret.append(r)
		return json.dumps(ret)

	@cherrypy.expose
	def addhost(self, host):
		logging.info('forced addition of %s' % host)
		do_pair.add(host)
		return 'will add %s' % host

	@cherrypy.expose
	def listhosts(self):
		ret = ''
		ret += '<html><body><table>'
		ret += '<tr><th>hostname</th><th>datetime</th><th>version</th><th>ether</th><th>inet</th><th>inet6</th><th>remote_addr</th><th>ops</th></tr>'
		for host in sorted(recs.keys()):
			rec = recs[host]
			ret += '<tr>'
			ret += '<td>%s</td>' % host
			ret += '<td>%s</td>' % dt_format(datetimes[host])
			ret += '<td>%s</td>' % rec['version']
			for af in ('ether', 'inet', 'inet6'):
				if af in rec:
					ret += '<td>'
					ret += '<br/>'.join(rec[af])
					ret += '</td>'
				else:
					ret += '<td></td>'
			ret += '<td>%s</td>' % rec['remote_addr']
			if host in unpaired:
				ret += '<td><a href="/addhost/?host=%s">add</a></td>' % host
			else:
				ret += '<td></td>'
			ret += '</tr>'
		ret += '</table></body></html>'
		return ret


def check_zone(zone, fn):
	logging.debug('check_zone')
	cmd = 'named-checkzone %s %s' % (zone, fn)
	try:
		out = subprocess.check_output(cmd, shell=True).decode()
	except subprocess.CalledProcessError:
		logging.exception(cmd)
		return False
	logging.debug(out)
	return True


def update_serial(serial_fn, out_fn):
	# let's copy the file first to retain all attributes
	cmd = 'cp -a %s %s' % (serial_fn, out_fn)
	subprocess.check_call(cmd, shell=True)

	serial_done = False

	serial_file = open(serial_fn, 'r')
	out_file = open(out_fn, 'w')

	for line in serial_file:
		if 'erial' in line:
			if not serial_done:
				#serial = re.match('.*[0-9]+.*', line)
				serial = re.search('(\d+)', line).group(0)
				serial = int(serial)
				line = line.replace(str(serial), str(serial + 1))
				out_file.write(line)
				serial_done = True
				logging.debug('serial: %s -> %s' % (serial, serial + 1))
		else:
			out_file.write(line)

	if not serial_done:
		logging.error('failed to update serial')

	serial_file.close()
	out_file.close()


def generate_bind_lines(rec, dt):
	ret = ''
	for af in ['inet', 'inet6']:
		if not af in rec:
			continue
		for a in rec[af]:
			if ipaddress.ip_address(a).is_private \
			or ipaddress.ip_address(a).is_loopback \
			or ipaddress.ip_address(a).is_link_local:
				continue
			dns_f = {'inet': 'a', 'inet6': 'aaaa'}[af]
			host = rec['hostname'].lower()
			#ttl = rec['ttl'].upper()
			ttl = '10M'
			dns_f = dns_f.upper()
			ret += '%s\t%s\t%s\t%s ; @faddns %s\n' % (host, ttl, dns_f, a, dt_format(dt))
			logging.debug('%s %s %s' % (host, af, a))
	return ret


def update_zone(zone_fn, out_fn, recs):
	written = set()

	# let's copy the file first to retain all attributes
	cmd = 'cp -a %s %s' % (zone_fn, out_fn)
	subprocess.check_call(cmd, shell=True)

	zone_file = open(zone_fn, 'r')
	out_file = open(out_fn, 'w')

	for line in zone_file:
		'''
		m = re.match('(\S+)\t(\S+)\t(\S+)\t(\S+).*', line)
		if not m:
			out_file.write(line)
			continue

		logging.debug(m.groups())
		m_host, m_ttl, m_typ, m_addr = m.groups()
		host = m_host.lower()
		'''

		if not '@faddns' in line:
			out_file.write(line)
			continue

		host = line.split()[0].lower()

		if host in written: continue

		if not host in changed:
			logging.debug('%s not in changes, skipping' % host)
			out_file.write(line)
			continue

		rec = recs[host]

		logging.info('updating %s' % host)
		written.add(host)
		changed.remove(host)

		out = generate_bind_lines(rec, datetimes[host])
		if out:
			out_file.write(out)
		else:
			logging.debug('change contains no usable data, keeping old record')
			out_file.write(line)

	# these are the unprocessed hosts
	for host in changed.copy():
		if not host in do_pair:
			continue
		rec = recs[host]
		logging.info('updating %s' % host)
		written.add(host)
		changed.remove(host)
		# TODO: most of this is cut-n-pasted from above
		out = generate_bind_lines(rec, datetimes[host])
		if out:
			out_file.write(out)
		else:
			logging.debug('change contains no usable data, keeping old record')
			out_file.write(line)

	zone_file.close()
	out_file.close()
	return changed - written


def sign_zone(zone, serial_fn):
	cmd = 'cd %s; dnssec-signzone -o %s %s' % (os.path.dirname(serial_fn), zone, serial_fn)
	subprocess.check_call(cmd, shell=True)


# TODO: rename to something better
def do_dns_update(zone, zone_fn, serial_fn, out_fn):
	global unpaired

	if not changed:
		logging.info('no changes found, doing nothing')
		return
	elif not do_pair and changed == unpaired:
		logging.info('only unforced hosts in changes, doing nothing')
		return

	unpaired = update_zone(zone_fn, out_fn, recs)

	if zone_fn != serial_fn:
		logging.info('zone file and serial file are not the same, skipping check')
	elif not check_zone(zone, out_fn):
		logging.error('zone check error!')
		return

	cmd = 'mv %s %s' % (out_fn, zone_fn)
	subprocess.check_call(cmd, shell=True)

	update_serial(serial_fn, out_fn)

	cmd = 'mv %s %s' % (out_fn, serial_fn)
	subprocess.check_call(cmd, shell=True)

	sign_zone(zone, serial_fn)

	cmd = 'rndc reload %s' % zone
	subprocess.check_call(cmd, shell=True)

	for host in changed:
		logging.warning('%s not processed!' % host)


# TODO: rename
def xxx():
	do_dns_update(g.zone, g.zone_fn, g.serial_fn, g.out_fn)


def logging_setup(level):
	logging.basicConfig(level=level)


def main():
	args = docopt.docopt(__doc__, version=__version__)

	debug = args['--debug']
	if debug:
		logging_setup('DEBUG')
	else:
		logging_setup('INFO')

	zone = args['<zone>']
	zone_fn = args['<zone_fn>']
	serial_fn = args['<serial_fn>']
	out_fn = '/tmp/%s.zone_tmp' % zone  # TODO: switch to tmpfile

	if not serial_fn:
		logging.info('no serial_fn specified, assuming it to be the same as zone_fn')
		serial_fn = zone_fn

	if args['--port']:
		port = int(args['--port'])
	else:
		port = 8765

	try:
		open(zone_fn, 'r').close()
	except:
		logging.critical('unable to open %s for reading' % zone_fn)
		return 1

	try:
		open(serial_fn, 'r').close()
	except:
		logging.critical('unable to open %s for reading' % serial_fn)
		return 1

	server = FADDNSServer()

	# TODO: this is ugly as hell!!!
	g.zone = zone
	g.zone_fn = zone_fn
	g.serial_fn = serial_fn
	g.out_fn = out_fn

	frequency = 60  # TODO: hard-coded shit

	m = Monitor(cherrypy.engine, xxx, frequency=frequency, name='Worker')  # TODO: rename
	m.subscribe()

	cherrypy.server.socket_host = '0.0.0.0'
	cherrypy.server.socket_port = port
	cherrypy.config.update({
		'engine.autoreload.on': False,
		'tools.proxy.on': True,  # retain the original address if we're being forwarded to
	})

	cherrypy.tree.mount(server, '/')
	cherrypy.engine.start()
	cherrypy.engine.block()

	return 0


if __name__ == '__main__':
	sys.exit(main())
