#!/usr/bin/python

__version__ = '0.0'

import cherrypy
import os
import os.path
import datetime

path_prefix = '/tmp'

class NsUpdateServer(object):
	@cherrypy.expose
	def index(self, version=None, host=None, domain=None, *args, **kwargs):
		dt = datetime.datetime.now().strftime('%Y-%m-%dT%H:%M:%S')
		remote_addr = cherrypy.request.remote.ip

		dn = '%s/%s' % (path_prefix, domain)
		if not os.path.isdir(dn): os.mkdir(dn)

		fn = '%s/%s' % (dn, host)
		f = open(fn, 'w')

		f.write('%s %s %s\n' % (version, dt, remote_addr))
		f.write('%s %s\n' % (host, domain))

		for af in 'ether', 'inet', 'inet6':
			if not af in kwargs: continue

			if isinstance(kwargs[af], unicode):
				addrs = (kwargs[af], )
			else:
				addrs = kwargs[af]
			#endif

			for a in addrs:
				f.write('%s %s\n' % (af, a))
			#endfor
		#endfor

		f.close()
	#enddef
#endclass

cherrypy.server.socket_host = '0.0.0.0'
cherrypy.server.socket_port = 8765
cherrypy.quickstart(NsUpdateServer())
