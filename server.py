#!/usr/bin/python3

'''
freakin' awesome dynamic dns server

Usage:
  faddnsd [options]

Options:
  -p <port>, --port=<port>        Port number.
'''

from version import __version__

import cherrypy
import datetime
import logging
import docopt
import json


class FADDNSServer(object):
	def __init__(self):
		self.recs = {}
	#enddef

	@cherrypy.expose
	def index(self, version=None, host=None, *args, **kwargs):
		if not host:
			logging.info('no host specified, ignoring')
			return 'no host specified'
		#endif

		rec = {}
		rec['version'] = version
		rec['host'] = host
		rec['datetime'] = datetime.datetime.now().strftime('%Y-%m-%dT%H:%M:%S')
		rec['remote_addr'] = cherrypy.request.remote.ip

		for af in 'ether', 'inet', 'inet6':
			if not af in kwargs: continue

			if isinstance(kwargs[af], str):
				rec[af] = [kwargs[af], ]
			else:
				rec[af] = kwargs[af]
			#endif
		#endfor

		self.recs[host] = rec

		return 'OK'
	#enddef

	@cherrypy.expose
	def dump(self):
		return json.dumps(self.recs, indent=4)
	#enddef
#endclass


def logging_setup(level):
	logging.basicConfig(level=level)
#enddef


def main():
	args = docopt.docopt(__doc__, version=__version__)
	 
	if args['--port']:
		port = int(args['--port'])
	else:
		port = 8765
	#endif

	logging_setup('DEBUG')

	cherrypy.server.socket_host = '0.0.0.0'
	cherrypy.server.socket_port = port
	cherrypy.quickstart(FADDNSServer())
#enddef


if __name__ == '__main__':
	main()
#enddef
