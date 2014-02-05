import socket
from configparser import ConfigParser

class Config:
	def __init__(self):
		self.domain = None
		self.host = socket.gethostname().lower()
		self.interval = 600
		self.url_prefix = ['http://faddns.podgorny.cz/']
	#enddef

	def read_from_ini(self, fn):
		ini = ConfigParser()
		ini.read(fn)

		self.domain = ini.get('General', 'Domain', fallback=self.domain)
		self.host = ini.get('General', 'Host', fallback=self.host)
		self.interval = ini.getint('General', 'Interval', fallback=self.interval)
		self.url_prefix = ini.get('General', 'UrlPrefix', fallback=self.url_prefix)
	#enddef

	def check(self):
		pass
	#enddef

	# TODO: move this to some common module
	def __str__(self):
		l = []

		for k, v in vars(self).items():
			l.append('%s=\'%s\'' % (k, v))
		#endfor

		return ', '.join(l)
	#enddef
#endclass

cfg = Config()
