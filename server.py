#!/usr/bin/python3

'''
freakin' awesome dynamic dns server

Usage:
  faddnsd [options] <work_dir>

Options:
  -p <port>, --port=<port>  Port number.
'''

from version import __version__

import cherrypy
import os
import datetime
import logging
import docopt
import json


class FADDNSServer(object):
	def __init__(self, path_prefix):
		self.path_prefix = path_prefix
	#enddef

	@cherrypy.expose
	def index(self, version=None, host=None, *args, **kwargs):
		rec = {}
		rec['version'] = version
		rec['host'] = host
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

		if not os.path.isdir(self.path_prefix):
			os.mkdir(self.path_prefix)
		#endif

		fn = '%s/%s' % (self.path_prefix, host)
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
	 
	path_prefix = args['<work_dir>']
	port = int(args['--port'])

	logging_setup('DEBUG')

	cherrypy.server.socket_host = '0.0.0.0'
	cherrypy.server.socket_port = port
	cherrypy.quickstart(FADDNSServer(path_prefix))
#enddef


if __name__ == '__main__':
	main()
#enddef
