#!/usr/bin/python3

import urllib
import urllib.request
import urllib.error
import urllib.parse
import ipaddress
import logging
import subprocess
import re
import time


def logging_setup(level, fn=None):
	logger = logging.getLogger()
	logger.setLevel(logging.DEBUG)

	formatter = logging.Formatter('%(levelname)s: %(message)s')
	sh = logging.StreamHandler()
	sh.setLevel(level)
	sh.setFormatter(formatter)
	logger.addHandler(sh)

	if fn:
		formatter = logging.Formatter('%(asctime)s: %(levelname)s: %(message)s')
		fh = logging.FileHandler(fn)
		fh.setLevel(level)
		fh.setFormatter(formatter)
		logger.addHandler(fh)


def call_OLD(cmd):
	logging.debug('calling: %s' % cmd)

	import os
	f = os.popen(cmd)
	ret = f.read()
	f.close()
	return ret


def call(cmd):
	logging.debug('calling: %s' % cmd)
	return subprocess.check_output(cmd, shell=True).decode('cp1250')


def get_addrs_windows():
	ret = {}

	# TODO: get ipv4 addresses

	lines = call('netsh interface ipv6 show address')

	for line in lines.split('\n'):
		if 'Temporary' in line: continue

		for word in line.split():
			word = word.strip().lower()

			if not ':' in word: continue
			if not word.startswith('200'): continue

			if not 'inet6' in ret: ret['inet6'] = set()
			ret['inet6'].add(word)

	# disable ether for now
	'''
	lines = call('ipconfig /all')
	for word in lines.split():
		word = word.strip().lower()
		if not re.match('..-..-..-..-..-..', word): continue

		word = word.replace('-', ':')

		if not 'ether' in ret: ret['ether'] = set()
		ret['ether'].add(word)
	'''

	return ret


def get_addrs_linux():
	ret = {}

	lines = call('ip addr').split('\n')

	for line in lines:
		line = line.strip()

		if not 'ether' in line \
		and not 'inet' in line:
			continue

		if 'temporary' in line: continue

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
			logging.error('unknown address type! (%s)' % addr_type)

		try:
			addr = addr.split('/')[0]
		except: pass

		if addr_type == 'ether':
			if addr == '00:00:00:00:00:00': continue
		elif addr_type == 'inet':
			if ipaddress.ip_address(addr).is_private: continue
			if ipaddress.ip_address(addr).is_loopback: continue
			if ipaddress.ip_address(addr).is_link_local: continue
		elif addr_type == 'inet6':
			if ipaddress.ip_address(addr).is_private: continue
			if ipaddress.ip_address(addr).is_loopback: continue
			if ipaddress.ip_address(addr).is_link_local: continue

		if not addr_type in ret: ret[addr_type] = set()
		ret[addr_type].add(addr)

	# disable ether for now
	if 'ether' in ret:
		del ret['ether']

	return ret


def send_addrs(url_prefix, host, version, addrs):
	# TODO: for the next version?
	#recs = []
	#for i in addrs:
	#	r = []
	#	for k,v in i.items(): r.append('%s=%s' % (k, v))
	#	r = ','.join(r)
	#	recs.append(r)
	#logging.debug('recs = %s' % recs)

	logging.debug('sending info to %s' % url_prefix)

	d = {
		'version': version,
		'host': host,
		#'records': recs
	}
	d.update(addrs)
	url = '%s?%s' % (url_prefix, urllib.parse.urlencode(d, True))

	logging.debug(url)

	try:
		u = urllib.request.urlopen(url).read().decode('utf-8')

		if 'OK' in ''.join(u):
			logging.debug('OK')
			return True
		else:
			logging.warning('got NOT OK')
			for i in u: logging.warning(i.strip())
	except urllib.error.URLError:
		logging.exception('urllib.request.urlopen() exception, probably failed to connect')

	return False


class MainLoop:
	def __init__(self, get_addrs_f, host, url, version, interval):
		self.get_addrs_f = get_addrs_f
		self.host = host
		self.url = url
		self.version = version
		self.interval = interval

		self._run = False
		self._refresh = False

	def run(self):
		logging.debug('main loop')

		addrs_old = None

		interval = 60  # TODO: hard-coded shit
		t_last = 0
		self._run = True
		while self._run:
			t = time.monotonic()

			if t - t_last > interval or self._refresh:
				addrs = self.get_addrs_f()
				logging.debug(str(addrs))

				if not addrs:
					logging.debug('no addresses, setting interval to 60')
					interval = 60  # TODO: hard-coded shit
				else:
					logging.debug('some addresses, setting interval to %s' % self.interval)
					interval = self.interval

				# disable this for now since we also want to use this as 'i am alive' signal
				#if self._refresh or addrs != addrs_old:
				if 1:
					logging.info('sending info to %s (%s)' % (self.url, addrs))
					if send_addrs(self.url, self.host, self.version, addrs):
						addrs_old = addrs
					else:
						logging.warning('send_addrs failed')
				else:
					logging.debug('no change, doing nothing')

				self._refresh = False
				t_last = t
			else:
				time.sleep(0.1)

		logging.debug('exited main loop')

	def stop(self):
		self._run = False

	def refresh(self):
		self._refresh = True
