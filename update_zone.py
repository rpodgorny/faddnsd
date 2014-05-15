#!/usr/bin/python3

'''
faddns zone updater.

Usage:
  zone_update <zone> <zone_fn> <serial_fn> <changes_dir>
'''

from version import __version__

import sys
import glob
import re
import subprocess
import os
import logging
import docopt
import json


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


class Change:
	def __init__(self):
		self.ttl = '10m'
		self.processed = False
	#enddef

	def read_from_file(self, fn):
		self.fn = fn

		f = open(fn, 'r')
		rec = json.loads(f.read())
		f.close()

		self.version = rec['version']
		self.datetime = rec['datetime']
		self.remote_addr = rec['remote_addr']
		self.host = rec['host']
		self.addrs = rec['addrs']
	#enddef
#endclass


def logging_setup(level):
	logging.basicConfig(level=level)
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
			if not line.startswith(i.host+'\t'): continue
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

		if change.processed: continue

		logging.info('updating %s' % change.host)

		#m_host, m_ttl, m_typ, m_addr = m.groups()
		#logging.debug(m)
		#logging.debug(m.groups())

		out = ''
		for af_a in change.addrs:
			af = af_a['af']
			a = af_a['a']

			if af == 'inet':
				af = 'a'
			elif af == 'inet6':
				af = 'aaaa'
			else:
				logging.debug('unsupported record type %s' % af)
				continue
			#endif

			host = change.host.lower()
			ttl = change.ttl.upper()
			af = af.upper()

			out += '%s\t%s\t%s\t%s ; %s\n' % (host, ttl, af, a, change.datetime)
			logging.info('%s %s' % (af, a))
		#endfor

		if out:
			out_file.write(out)
			change.processed = True
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
	changes_dir = args['<changes_dir>']
	out_fn = '/tmp/%s.zone_tmp' % zone

	logging_setup('DEBUG')

	changes = []
	for i in glob.glob(changes_dir+'/*'):
		if not os.path.isfile(i): continue

		c = Change()
		c.read_from_file(i)
		changes.append(c)
	#endfor

	if not changes:
		logging.info('no change files found, doing nothing')
		return
	#endif

	for i in changes:
		logging.debug('%s %s' % (i.host, i.addrs))
	#endfor
	
	update_zone(zone_fn, out_fn, changes)

	if not check_zone(zone, out_fn):
		logging.error('zone check error!')
		return
	#endif

	cmd = 'mv %s %s' % (out_fn, zone_fn)
	subprocess.call(cmd, shell=True)

	update_serial(serial_fn, out_fn)

	cmd = 'mv %s %s' % (out_fn, serial_fn)
	subprocess.call(cmd, shell=True)

	cmd = 'rndc reload %s' % zone
	subprocess.call(cmd, shell=True)

	for c in changes:
		if c.processed:
			os.remove(c.fn)
			continue
		#endif

		logging.warning('%s not processed!' % c.host)
	#endfor
#enddef


if __name__ == '__main__': main()
