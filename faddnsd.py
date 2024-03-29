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
  --no-zone-reload          Don't reload zone file (by invoking rndc).
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
import threading


# TODO: global shit
g = {}
recs = {}
datetimes = {}
ts = {}
changed = set()
unpaired = set()
do_pair = set()
t_zone = None


def dt_format(dt):
	return dt.strftime('%Y-%m-%d %H:%M:%S')


def call(cmd):
	print("+ %s" % cmd)
	return subprocess.check_call(cmd, shell=True)


class FADDNSServer(object):
	@cherrypy.expose
	def index(self, version=None, host=None, *args, **kwargs):
		if not host:
			logging.debug('no host specified, ignoring')
			return '''
<html>
<body>
<p>no host specified</p>
<p><a href="listhosts">listhosts</a></p>
<p><a href="dump">dump</a></p>
</body>
</html>
'''
		rec = {
			'hostname': host,
			'version': version,
			'remote_addr': cherrypy.request.remote.ip,
		}
		for af in ['ether', 'inet', 'inet6']:
			if af not in kwargs:
				continue
			rec[af] = set([kwargs[af]]) if isinstance(kwargs[af], str) else set(kwargs[af])
		logging.debug("rec: %s" % rec)
		if rec != recs.get(host):
			logging.debug("rec change: %s" % rec)
			recs[host] = rec
			changed.add(host)
		datetimes[host] = datetime.datetime.now()
		ts[host] = time.time()
		return 'OK'

	@cherrypy.expose
	def dump(self):
		for host, rec in recs.items():
			r = rec
			r.update({
				'datetime': dt_format(datetimes[host]),
				't': ts[host],
			})
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
			r.update({
				'datetime': dt_format(datetimes[host]),
				't': ts[host],
			})
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
	logging.debug("update_serial %s %s" % (serial_fn, out_fn))
	call('cp -a %s %s' % (serial_fn, out_fn))  # let's copy the file first to retain all attributes
	serial_done = False
	with open(serial_fn, 'r') as serial_file, open(out_fn, 'w') as out_file:
		for line in serial_file:
			if 'erial' in line:
				if not serial_done:
					#serial = re.match('.*[0-9]+.*', line)
					serial = re.search('(\d+)', line).group(0)
					serial = int(serial)
					line = line.replace(str(serial), str(serial + 1))
					out_file.write(line)
					serial_done = True
					logging.info('%s serial: %s -> %s' % (serial_fn, serial, serial + 1))
			else:
				out_file.write(line)
		if not serial_done:
			logging.error('failed to update serial')


def generate_bind_lines(rec, dt):
	ret = ''
	for af in ['inet', 'inet6']:
		if af not in rec:
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


def update_zone(zone_fn, out_fn, recs, changed):
	written = set()

	# let's copy the file first to retain all attributes
	call('cp -a %s %s' % (zone_fn, out_fn))

	with open(zone_fn, 'r') as zone_file, open(out_fn, 'w') as out_file:
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

			if '@faddns' not in line:
				out_file.write(line)
				continue
			host = line.split()[0].lower()
			if host in written:
				continue
			if host not in changed:
				logging.debug('%s not in changes, skipping' % host)
				out_file.write(line)
				continue

			rec = recs[host]

			logging.info('updating %s' % host)
			written.add(host)
			changed.remove(host)  # TODO: fucking side effect!

			out = generate_bind_lines(rec, datetimes[host])
			if out:
				out_file.write(out)
			else:
				logging.debug('change contains no usable data, keeping old record')
				out_file.write(line)

		# these are the unprocessed hosts
		for host in changed.copy():
			if host not in do_pair:
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

	return changed - written


# TODO: rename to something better
def do_dns_update(changed, zone, zone_fn, serial_fn, out_fn, no_zone_reload=False):
	global unpaired

	if not changed:
		logging.debug('no changes found, doing nothing')
		return
	elif not do_pair and changed == unpaired:
		logging.debug('only unforced hosts in changes, doing nothing')
		return

	unpaired = update_zone(zone_fn, out_fn, recs, changed)

	if zone_fn != serial_fn:
		logging.debug('zone file and serial file are not the same, skipping check')
	elif not check_zone(zone, out_fn):
		logging.error('zone check error!')
		return

	assert os.path.getsize(out_fn) > 10  # TODO: hard-coded shit

	call('mv %s %s' % (out_fn, zone_fn))

	update_serial(serial_fn, out_fn)

	assert os.path.getsize(out_fn) > 10  # TODO: hard-coded shit

	call('mv %s %s' % (out_fn, serial_fn))

	call('cd %s; dnssec-signzone -o %s %s' % (os.path.dirname(serial_fn), zone, serial_fn))

	for host in changed.copy():
		logging.warning('%s not processed!' % host)

	if not no_zone_reload:
		call('rndc reload %s' % zone)


# TODO: rename
_run = 1
def the_loop():
	global g
	global changed
	global ts
	t_last = 0
	while _run:
		t = time.time()
		ts_max = max(ts.values()) if ts else None
		#if (ts_max and (t - ts_max > 5)) or (t - t_last > 30):  # TODO: hard-coded shit
		if t - t_last > 30:  # TODO: hard-coded shit
			print("HOVNO", t, t_last, ts_max)
			t_last = t
			do_dns_update(changed, g["zone"], g["zone_fn"], g["serial_fn"], g["out_fn"], g["no_zone_reload"])
		time.sleep(1)


def logging_setup(level):
	logging.basicConfig(level=level)


def main():
	args = docopt.docopt(__doc__, version=__version__)

	debug = args["--debug"]
	logging_setup("DEBUG" if debug else "INFO")
	if not debug:
		# TODO: none of this shit seems to be working :-( - ...well, is does. ...but which one? i don't care for now...
		cherrypy.log.access_log.propagate = False
		cherrypy.config.update({'log.access_file': ''})
		cherrypy.config.update({'log.screen': False})

	zone = args['<zone>']
	zone_fn = args['<zone_fn>']
	serial_fn = args['<serial_fn>']
	no_zone_reload = args["--no-zone-reload"]
	out_fn = '/tmp/%s.zone_tmp' % zone

	if not serial_fn:
		logging.info('no serial_fn specified, assuming it to be the same as zone_fn')
		serial_fn = zone_fn

	port = int(args['--port']) if args['--port'] else 80

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
	global g
	g = {
		"zone": zone,
		"zone_fn": zone_fn,
		"serial_fn": serial_fn,
		"out_fn": out_fn,
		"no_zone_reload": no_zone_reload,
	}

	#frequency = 60  # TODO: hard-coded shit
	#m = Monitor(cherrypy.engine, xxx, frequency=frequency, name='Worker')  # TODO: rename
	#m.subscribe()
	thr = threading.Thread(target=the_loop)
	thr.start()

	cherrypy.server.socket_host = '0.0.0.0'
	cherrypy.server.socket_port = port
	cherrypy.config.update({
		'engine.autoreload.on': False,
		'tools.proxy.on': True,  # retain the original address if we're being forwarded to
	})

	cherrypy.tree.mount(server, '/')
	cherrypy.engine.start()
	cherrypy.engine.block()

	global _run
	_run = 0
	thr.join()

	return 0


if __name__ == '__main__':
	sys.exit(main())
