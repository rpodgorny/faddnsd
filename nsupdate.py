#!/usr/bin/python3


import socket
import urllib
import urllib.request
import urllib.error
import urllib.parse
import ipaddress
import re
import os.path

import log


# TODO: uglyyy!!!
_run = True


def call(cmd):
	log.debug('calling: %s' % cmd)

	import os
	f = os.popen(cmd)
	return f.read()
#enddef


def get_addrs_windows():
	ret = []

	lines = call('netsh interface ipv6 show address')

	for line in lines.split('\n'):
		print(line)
		if 'Temporary' in line: continue

		for word in line.split():
			word = word.strip().lower()

			if not ':' in word: continue
			if not word.startswith('200'): continue

			ret.append({'af': 'inet6', 'a': word})
		#endfor
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
			log.error('unknown address type! (%s)' % addr_type)
		#endif

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
		#endif

		ret.append({'af': addr_type, 'a': addr})
	#endfor

	return ret
#enddef


def send_addrs(url_prefix, host, domain, version, addrs):
	# TODO: for the next version?
	#recs = []
	#for i in addrs:
	#	r = []
	#	for k,v in i.items(): r.append('%s=%s' % (k, v))
	#	r = ','.join(r)
	#	recs.append(r)
	#endfor
	#log.debug('recs = %s' % recs)

	a = {'ether': [], 'inet': [], 'inet6': []}
	for i in addrs:
		if not i['af'] in a: continue
		a[i['af']].append(i['a'])
	#endfor

	d = {
		'version': version,
		'host': host,
		'domain': domain,
		#'records': recs
	}
	d.update(a)
	url = '%s?%s' % (url_prefix, urllib.parse.urlencode(d, True))

	log.debug(url)

	try:
		u = urllib.request.urlopen(url).read().decode('utf-8')

		if 'OK' in ''.join(u):
			log.debug('OK')
		else:
			log.warning('NOT OK')
			for i in u: log.warning(i.strip())
		#endif
	except urllib.error.URLError:
		log.error('urllib.request.urlopen() exception, probably failed to connect')
		log.log_exc()
	#endtry
#enddef
