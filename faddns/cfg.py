import socket
from configparser import ConfigParser


class Config:
	def __init__(self):
		self.host = socket.gethostname().lower()
		self.interval = 600
		self.url = None

	def read_from_ini(self, fn):
		ini = ConfigParser()
		ini.read(fn)

		self.host = ini.get('General', 'Host', fallback=self.host)
		self.interval = ini.getint('General', 'Interval', fallback=self.interval)
		self.url = ini.get('General', 'Url', fallback=self.url)

	def check(self):
		if not self.url: return 'no url!'

	# TODO: move this to some common module
	def __str__(self):
		l = []
		for k, v in vars(self).items():
			l.append('%s=\'%s\'' % (k, v))
		return ', '.join(l)


cfg = Config()
