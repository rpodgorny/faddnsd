#!/usr/bin/python2

__version__ = '1.0'

import sys
import socket
import urllib
import time
import getopt
import re
from iniparser import IniParser
from SimpleXMLRPCServer import SimpleXMLRPCServer

import log
sys.excepthook = log.log_exception
log.filename = 'nsupdate.log'

# TODO: uglyyy!!!
_run = True

class Config:
	def __init__(self):
		self.domain = None
		self.host = socket.gethostname().lower()
		self.interval = 600
		self.url_prefix = []
	#enddef

	def read_from_ini(self, fn):
		ini = IniParser()
		ini.read(fn)

		self.domain = ini.get('General', 'Domain', self.domain)
		self.host = ini.get('General', 'Host', self.host)
		self.interval = ini.getint('General', 'Interval', self.interval)
		self.url_prefix = ini.get('General', 'UrlPrefix', self.url_prefix)
	#enddef

	def getopt(self, argv):
		opts, args = getopt.getopt(argv, 'd:h:i:u:', ('domain=', 'host=', 'interval=', 'url-prefix='))
		for o,a in opts:
			if o in ('-d', '--domain'):
				self.domain = a.lower()
			elif o in ('-h', '--host'):
				self.host = a.lower()
			elif o in ('-i', '--interval'):
				self.interval = int(a)
			elif o in ('-u', '--url-prefix'):
				self.url_prefix.append(a)
			#endif
		#endfor

		if not self.url_prefix: self.url_prefix = ['http://podgorny.cz:8765/', ]
	#enddef

	def check(self):
		if not self.domain: return 'domain not specified!'
	#enddef
	
	# TODO: move this to some common module
	def __str__(self):
		l = []

		for k,v in vars(self).items():
			l.append('%s=\'%s\'' % (k, v))
		#endfor

		return ', '.join(l)
	#enddef 
#endclass

cfg = Config()

# TODO: this is disabled because it does not work when compiled as windows application
def call_old(cmd):
	log.log('calling: %s' % cmd)
	
	import subprocess

	try:
		return subprocess.check_output(cmd, shell=True)
	except AttributeError:
		# python < 2.7
		p = subprocess.Popen(cmd, shell=True, stdout=subprocess.PIPE)
		p.wait()
		return p.communicate()[0]
	#endtry
#enddef

def call(cmd):
	log.log('calling: %s' % cmd)

	import os
	f = os.popen(cmd)
	return f.read()
#enddef

def get_addrs_windows():
	ret = []

	lines = call('netsh interface ipv6 show address')

	for word in lines.split():
		word = word.strip().lower()

		if not ':' in word: continue
		if not word.startswith('200'): continue

		ret.append({'af': 'inet6', 'a': word})
	#endfor
	
	lines = call('ipconfig /all')
	for word in lines.split():
		word = word.strip().lower()
		if not re.match('..-..-..-..-..-..', word): continue

		word = word.replace('-', ':')
		ret.append({'af': 'ether', 'a': word})
	#endfor
	
	return ret
#enddef

def get_addrs_linux():
	ret = []

	lines = call('ip addr').split('\n')

	for line in lines:
		line = line.strip()

		if not 'ether' in line \
		and not 'inet' in line:
			continue
		#endif

		addr_type, addr, _ = line.split(' ', 2)
		addr_type = addr_type.lower()
		addr = addr.lower()

		if 'ether' in addr_type:
			addr_type = 'ether'
		elif 'inet6' in addr_type:
			addr_type = 'inet6'
		elif 'inet' in addr_type:
			addr_type = 'inet'
		else:
			log.log('unknown address type!')
		#endif

		try:
			addr = addr.split('/')[0]
		except: pass

		if addr_type == 'ether':
			if addr == '00:00:00:00:00:00': continue
		elif addr_type == 'inet':
			if addr.startswith('127.'): continue
			if addr.startswith('10.'): continue
			if addr.startswith('192.168.'): continue
		elif addr_type == 'inet6':
			if addr.startswith('::1'): continue
			if addr.startswith('fe80:'): continue
		#endif

		ret.append({'af': addr_type, 'a': addr})
	#endfor

	return ret
#enddef

class XMLRPCServer(object):
	def exit(self):
		log.log('xmlrcp: exit')
		global _run
		_run = False
	#enddef
#endclass

def init_xmlrpc():
	log.log('starting xmlrpc')

	server = SimpleXMLRPCServer(('localhost', 8889), allow_none=True, logRequests=False)
	server.register_introspection_functions()
	
	s = XMLRPCServer()
	server.register_instance(s)
	
	import thread
	thread.start_new_thread(server.serve_forever, ())
#enddef

def main():
	log.log('*' * 40)
	log.log('starting nsupdate v%s' %  __version__)
	
	cfg.read_from_ini('nsupdate.ini')

	cfg.getopt(sys.argv[1:])
	err = cfg.check()
	if err:
		log.log(err)
		return
	#endif
	
	log.log('%s' % cfg)

	if sys.platform == 'win32':
		log.log('detected win32')
		get_addrs = get_addrs_windows
	elif sys.platform == 'linux2':
		log.log('detected linux2')
		get_addrs = get_addrs_linux
	else:
		log.log('unknown platform!')
		return
	#endif
	
	init_xmlrpc()

	addr_life = {}

	try:
		global _run
		while _run:
			t = time.time()

			addrs = get_addrs()
			log.log(str(addrs))

			for url in cfg.url_prefix:
				# TODO: for the next version?
				#recs = []
				#for i in addrs:
				#	r = []
				#	for k,v in i.items(): r.append('%s=%s' % (k, v))
				#	r = ','.join(r)
				#	recs.append(r)
				#endfor
				#log.log('recs = %s' % recs)

				a = {'ether': [], 'inet': [], 'inet6': []}
				for i in addrs:
					if not i['af'] in a: continue
					a[i['af']].append(i['a'])
				#endfor

				d = {
					'version': __version__,
					'host': cfg.host,
					'domain': cfg.domain,
					#'records': recs
				}
				d.update(a)
				url += '?' + urllib.urlencode(d, True)

				log.log(url)

				try:
					u = urllib.urlopen(url)

					if 'OK' in ''.join(u):
						log.log('OK')
					else:
						log.log('NOT OK')
						for i in u: log.log(i.strip())
					#endif
				except:
					log.log_exc()
				#endtry
			#endfor

			log.log('sleeping for %ss' % cfg.interval)
			while time.time() - t < cfg.interval:
				if not _run: break
				time.sleep(1)
			#endwhile
		#endwhile
	except KeyboardInterrupt:
		log.log('keyboard interrupt!')
	#endtry
	
	log.log('exited main loop')
#enddef

if __name__ == '__main__': main()
