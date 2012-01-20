#!/usr/bin/python

__version__ = '0.2'

import sys
import socket
import subprocess
import urllib
import time
import getopt
import re

class Config:
	def __init__(self):
		self.domain = None
		self.host = socket.gethostname().lower()
		self.interval = 600
		self.url_prefix = []
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

		if not self.url_prefix: self.url_prefix = ['http://wiki.asterix.cz/ip.php', ]
	#enddef

	def check(self):
		if not self.domain: return 'domain not specified!'
	#enddef
#endclass

cfg = Config()

def call(cmd):
	try:
		return subprocess.check_output(cmd, shell=True)
	except AttributeError:
		# python < 2.7
		p = subprocess.Popen(cmd, shell=True, stdout=subprocess.PIPE)
		p.wait()
		return p.communicate()[0]
	#endtry
#enddef

def get_addrs_windows():
	ret = {'ether': [], 'inet': [], 'inet6': []}

	lines = call('netsh interface ipv6 show address')

	for word in lines.split():
		word = word.strip().lower()

		if not ':' in word: continue
		if not word.startswith('200'): continue

		ret['inet6'].append(word)
	#endfor
	
	lines = call('ipconfig /all')
	for word in lines.split():
		word = word.strip().lower()
		if not re.match('..-..-..-..-..-..', word): continue

		word = word.replace('-', ':')
		ret['ether'].append(word)
	#endfor
	
	return ret
#enddef

def get_addrs_linux():
	ret = {'ether': [], 'inet': [], 'inet6': []}

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
			print 'unknown address type!'
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

		ret[addr_type].append(addr)
	#endfor

	return ret
#enddef

# TODO: uglyyy!
exit = False
tb = None

def tray():
	import wx

	class Tray(wx.TaskBarIcon):
		def CreatePopupMenu(self):
			menu = wx.Menu()
			menu.Append(123, 'exit')
			self.Bind(wx.EVT_MENU, self.on_exit, id=123)
			return menu
		#enddef

		def on_exit(self, e):
			global exit
			exit = True
			print 'exit'
		#enddef
	#endclass

	app = wx.App(0)
	icon = wx.Icon('icon.jpg', wx.BITMAP_TYPE_JPEG)
	global tb
	tb = Tray()
	tb.SetIcon(icon, 'nsupdate')

	app.MainLoop()
#enddef

def main():
	print 'nsupdate v%s' % __version__

	cfg.getopt(sys.argv[1:])
	err = cfg.check()
	if err:
		print err
		return
	#endif

	if sys.platform == 'win32':
		print 'detected win32'
		get_addrs = get_addrs_windows
		
		import thread
		thread.start_new_thread(tray, ())
	elif sys.platform == 'linux2':
		print 'detected linux2'
		get_addrs = get_addrs_linux
	else:
		print 'unknown platform!'
		return
	#endif
	
	addr_life = {}

	while not exit:
		t = time.time()

		addrs = get_addrs()
		print addrs
		
		for url in cfg.url_prefix:
			d = {
				'version': __version__,
				'host': cfg.host,
				'domain': cfg.domain
			}
			d.update(addrs)
			url += '?' + urllib.urlencode(d, True)
			print url

			try:
				u = urllib.urlopen(url)
				#for i in u: print i.strip()
				if 'OK' in ''.join(u):
					print 'OK'
				else:
					print 'NOT OK'
					for i in u: print i.strip()
				#endif
			except:
				print 'urllib exception!'
			#endtry
		#endfor

		print 'sleeping for %ss' % cfg.interval
		while time.time() - t < cfg.interval:
			if exit: break
			time.sleep(1)
		#endwhile
	#endwhile
	
	if sys.platform == 'win32':
		tb.Destroy()
	#endif
#enddef

if __name__ == '__main__': main()
