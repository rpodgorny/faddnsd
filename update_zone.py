#!/usr/bin/python3

'''
faddns zone updater.

Usage:
  zone_update <zone> <zone_fn> <serial_fn> <faddns_server_url>
'''

from version import __version__

import re
import subprocess
import logging
import docopt
import json
import urllib.request


def check_zone(zone, fn):
	cmd = 'named-checkzone %s %s' % (zone, fn) 

	try:
		out = subprocess.check_output(cmd, shell=True).decode()
	except subprocess.CalledProcessError:
		logging.exception(cmd)
		return False
	#endtry

	logging.debug(out)
	return True
#enddef


def logging_setup(level):
	logging.basicConfig(level=level)
#enddef


def get_changes(url):
	url += '/dump'
	logging.debug('getting changes from %s' % url)

	data = urllib.request.urlopen(url).read().decode()
	changes = json.loads(data)
	if changes:
		return changes.values()
	#endif

	return None
#enddef


def update_serial(serial_fn, out_fn):
	cmd = 'cp -a %s %s' % (serial_fn, out_fn)
	subprocess.call(cmd, shell=True)

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
			#endif
		else:
			out_file.write(line)
		#endif
	#endfor

	if not serial_done:
		logging.error('failed to update serial')
	#endif

	serial_file.close()
	out_file.close()
#enddef


def update_zone(zone_fn, out_fn, changes):
	serial_done = False

	cmd = 'cp -a %s %s' % (zone_fn, out_fn)
	subprocess.call(cmd, shell=True)

	zone_file = open(zone_fn, 'r')
	out_file = open(out_fn, 'w')

	for line in zone_file:
		change = None
		for i in changes:
			if not line.startswith(i['host']+'\t'): continue
			change = i
			break
		#endfor

		# no match
		if change is None:
			out_file.write(line)
			continue
		#endif

		m = re.match('(\S+)\t(\S+)\t(\S+)\t(\S+)', line)
		if not m:
			logging.debug('record for \'%s\' in wrong format, skipping' % line)
			out_file.write(line)
			continue
		#endif

		if 'processed' in change and change['processed']: continue

		logging.info('updating %s' % change['host'])

		#m_host, m_ttl, m_typ, m_addr = m.groups()
		#logging.debug(m)
		#logging.debug(m.groups())

		out = ''
		for af in ['inet', 'inet6']:
			if not af in change: continue

			for a in change[af]:
				dns_f = {'inet': 'a', 'inet6': 'aaaa'}[af]

				host = change['host'].lower()
				#ttl = change['ttl'].upper()
				ttl = '10m'
				dns_f = dns_f.upper()

				out += '%s\t%s\t%s\t%s ; %s\n' % (host, ttl, dns_f, a, change['datetime'])
				logging.info('%s %s' % (af, a))
			#endfor
		#endfor

		if out:
			out_file.write(out)
			change['processed'] = True
		else:
			logging.debug('change contains no usable data, keeping old record')
			out_file.write(line)
		#endif
	#endfor

	zone_file.close()
	out_file.close()
#enddef


def main():
	args = docopt.docopt(__doc__, version=__version__)

	zone = args['<zone>']
	zone_fn = args['<zone_fn>']
	serial_fn = args['<serial_fn>']
	faddns_server_url = args['<faddns_server_url>']
	out_fn = '/tmp/%s.zone_tmp' % zone

	logging_setup('DEBUG')

	changes = get_changes(faddns_server_url)

	if not changes:
		logging.info('no changes found, doing nothing')
		return
	#endif

	update_zone(zone_fn, out_fn, changes)

	#if not check_zone(zone, out_fn):
	#	logging.error('zone check error!')
	#	return
	#endif

	cmd = 'mv %s %s' % (out_fn, zone_fn)
	subprocess.call(cmd, shell=True)

	update_serial(serial_fn, out_fn)

	cmd = 'mv %s %s' % (out_fn, serial_fn)
	subprocess.call(cmd, shell=True)

	cmd = 'rndc reload %s' % zone
	subprocess.call(cmd, shell=True)

	for c in changes:
		if 'processed' in c and c['processed']:
			continue
		#endif

		logging.warning('%s not processed!' % c['host'])
	#endfor
#enddef


if __name__ == '__main__':
	main()
#endif
