#!/usr/bin/python

__version__ = '0.0'

import sys
import socket
import subprocess
import urllib

host = socket.gethostname()
domain = 'podgorny.cz'

def get_addrs_windows():
	lines = subprocess.check_output('netsh interface ipv6 show address')

	for word in lines.split():
		if not ':' in word: continue
		if not word.startswith('200'): continue

		yield 'aaaa', word
	#endfor
#enddef

def get_addrs_linux():
	lines = subprocess.check_output('ip addr', shell=True).split('\n')

	for line in lines:
		line = line.strip()
		if not line.startswith('inet'): continue

		addr_type, addr, _ = line.split(' ', 2)

		if addr_type == 'inet':
			addr_type = 'a'
		elif addr_type == 'inet6':
			addr_type = 'aaaa'
		else:
			print 'unknown address type!'
		#endif

		try:
			addr = addr.split('/')[0]
		except: pass

		if addr.startswith('127.'): continue
		if addr.startswith('10.'): continue
		if addr.startswith('192.168.'): continue
		if addr.startswith('::1'): continue
		if addr.startswith('fe80:'): continue

		yield addr_type, addr
	#endfor
#enddef

def main():
	if sys.platform == 'win32':
		addrs = get_addrs_windows()
	elif sys.platform == 'linux2':
		addrs = get_addrs_linux()
	else:
		print 'unknown platform!'
		return
	#endif

	tmp = []
	for af,a in addrs: tmp.append('%s=%s' % (af, a))
	addrs = tmp

	addrs = ','.join(addrs)

	url = 'http://wiki.asterix.cz/ip.php'
	url += '?' + urllib.urlencode({'host': host, 'domain': domain, 'addrs': addrs})
	print url

	u = urllib.urlopen(url)

	for i in u: print i.strip()
#enddef

if __name__ == '__main__': main()
