#!/usr/bin/python

__version__ = '0.0'

import sys
import glob
import re
import subprocess
import os

def check_zone(zone, fn):
	cmd = 'named-checkzone %s %s' % (zone, fn) 

	try:
		out = subprocess.check_output(cmd, shell=True)
	except subprocess.CalledProcessError:
		print cmd
		return False
	#endtry

	print out
	return True
#enddef

class Change:
	def __init__(self):
		self.ttl = '1h'
		self.processed = False
	#enddef

	def read_from_file(self, fn):
		self.fn = fn

		f = open(fn, 'r')
		self.version, self.date, self.time, self.remote_addr = f.readline().split()
		self.host, self.domain = f.readline().split()

		self.addrs = []
		for line in f:
			af, a = line.split()
			self.addrs.append((af, a))
		#endfor

		f.close()
	#enddef
#endclass

def main():
	serial_done = False

	zone = sys.argv[1]
	zone_fn = sys.argv[2]
	changes_dir = sys.argv[3]

	out_fn = '/tmp/tmp.zone' # TODO: rename variable and make the value random

	changes = []
	for i in glob.glob(changes_dir+'/*'):
		c = Change()
		c.read_from_file(i)
		changes.append(c)
	#endfor

	if not changes:
		print 'no change files found, doing nothing'
		return
	#endif
	
	cmd = 'cp -a %s %s' % (zone_fn, out_fn)
	subprocess.call(cmd, shell=True)

	zone_file = open(zone_fn, 'r')
	out_file = open(out_fn, 'w')

	for line in zone_file:
		if 'erial' in line:
			if not serial_done:
				#serial = re.match('.*[0-9]+.*', line)
				serial = re.search('(\d+)', line).group(0)
				serial = int(serial)
				line = line.replace(str(serial), str(serial+1))

				out_file.write(line)

				serial_done = True

				print 'serial: %s -> %s' % (serial, serial+1)
			#endif

			continue
		#endif

		change = None
		for i in changes:
			if not line.startswith(i.host): continue
			if i.domain != zone: continue
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
			print 'record for \'%s\' in wrong format, skipping' % line
			out_file.write(line)
			continue
		#endif

		if change.processed: continue

		print 'updating %s' % change.host

		#m_host, m_ttl, m_typ, m_addr = m.groups()
		#print m
		#print m.groups()

		for af,a in change.addrs:
			host = change.host.lower()
			ttl = change.ttl.upper()
			af = af.upper()

			if af not in ('A', 'AAAA'):
				print 'unsupported record type %s' % af
				continue
			#endif

			out_file.write('%s\t%s\t%s\t%s ; %s %s\n' % (host, ttl, af, a, change.date, change.time))
			print '%s %s' % (af, a)
		#endfor

		change.processed = True
	#endfor

	zone_file.close()
	out_file.close()

	if not check_zone(zone, zone_fn):
		print 'zone check error!'
		return
	#endif

	cmd = 'mv %s %s' % (out_fn, zone_fn)
	subprocess.call(cmd, shell=True)

	cmd = 'rndc reload %s' % zone
	subprocess.call(cmd, shell=True)

	for c in changes:
		if c.processed:
			os.remove(c.fn)
			continue
		#endif

		print '%s not processed!' % c.host
	#endfor
#enddef

if __name__ == '__main__': main()
