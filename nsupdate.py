#!/usr/bin/python

__version__ = '0.0'

import sys
import socket
import subprocess
import urllib
import time
import getopt


class Config:
	def __init__(self):
		self.domain = None
		self.host = socket.gethostname()
		self.interval = 600
		self.url_prefix = 'http://wiki.asterix.cz/ip.php'
	#enddef

	def getopt(self, argv):
		opts, args = getopt.getopt(argv, 'd:h:i:u:', ('domain=', 'host=', 'interval=', 'url-prefix='))
		for o,a in opts:
			if o in ('-d', '--domain'):
				self.domain = a
			elif o in ('-h', '--host'):
				self.host = a
			elif o in ('-i', '--interval'):
				self.interval = int(a)
			elif o in ('-u', '--url-prefix'):
				self.url_prefix = a
			#endif
		#endfor
	#enddef

	def check(self):
		if not self.domain: return 'domain not specified!'
	#enddef
#endclass

cfg = Config()

def get_addrs_windows():
	ret = []

	lines = subprocess.check_output('netsh interface ipv6 show address')

	for word in lines.split():
		if not ':' in word: continue
		if not word.startswith('200'): continue

		ret.append(('aaaa', word))
	#endfor
	
	return ret
#enddef

def get_addrs_linux():
	ret = []

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

		ret.append((addr_type, addr))
	#endfor
	
	return ret
#enddef

def main():
	cfg.getopt(sys.argv[1:])
	err = cfg.check()
	if err:
		print err
		return
	#endif

	if sys.platform == 'win32':
		print 'detected win32'
		get_addrs = get_addrs_windows
	elif sys.platform == 'linux2':
		print 'detected linux2'
		get_addrs = get_addrs_linux
	else:
		print 'unknown platform!'
		return
	#endif

	while 1:
		addrs = get_addrs()
		for af,a in addrs: print af,a

		tmp = []
		for af,a in addrs: tmp.append('%s=%s' % (af, a))
		addrs = ','.join(tmp)

		url = cfg.url_prefix
		url += '?' + urllib.urlencode({'host': cfg.host, 'domain': cfg.domain, 'addrs': addrs})
		print url

		u = urllib.urlopen(url)
		#for i in u: print i.strip()
		if 'OK' in ''.join(u):
			print 'OK'
		else:
			print 'NOT OK'
			for i in u: print i.strip()
		#endif

		print 'sleeping for %ss' % cfg.interval
		time.sleep(cfg.interval)
	#endwhile
#enddef

if __name__ == '__main__': main()
