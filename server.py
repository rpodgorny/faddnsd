#!/usr/bin/python3

'''
freakin' awesome dynamic dns server

Usage:
  faddnsd [options]

Options:
  -p <port>, --port=<port>  Port number.
'''

__version__ = '0.0'


import cherrypy
import os
import datetime
import logging
import docopt
import json


class NsUpdateServer(object):
	def __init__(self, path_prefix):
		self.path_prefix = path_prefix
	#enddef

	@cherrypy.expose
	def index(self, version=None, host=None, domain=None, *args, **kwargs):
		rec = {}
		rec['version'] = version
		rec['host'] = host
		rec['domain'] = domain
		rec['datetime'] = datetime.datetime.now().strftime('%Y-%m-%dT%H:%M:%S')
		rec['remote_addr'] = cherrypy.request.remote.ip

		rec['addrs'] = []
		for af in 'ether', 'inet', 'inet6':
			if not af in kwargs: continue

			if isinstance(kwargs[af], str):
				addrs = (kwargs[af], )
			else:
				addrs = kwargs[af]
			#endif

			for a in addrs:
				rec['addrs'].append({'af': af, 'a': a})
			#endfor
		#endfor

		dn = '%s/%s' % (self.path_prefix, domain)
		if not os.path.isdir(dn):
			os.mkdir(dn)
		#endif

		fn = '%s/%s' % (dn, host)
		f = open(fn, 'w')
		f.write(json.dumps(rec))
		f.close()

		return 'OK'
	#enddef
#endclass


def logging_setup(level):
	logging.basicConfig(level=level)
#enddef


def main():
	args = docopt.docopt(__doc__, version=__version__)
	 
	path_prefix = '/tmp'
	port = int(args['--port'])

	logging_setup('DEBUG')

	cherrypy.server.socket_host = '0.0.0.0'
	cherrypy.server.socket_port = port
	cherrypy.quickstart(NsUpdateServer(path_prefix))
#enddef


if __name__ == '__main__':
	main()
#enddef
