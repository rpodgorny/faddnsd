from distutils.core import setup
import py2exe

from version import __version__


setup(
	console = ['nsupdate', ],
	version = __version__,
	zipfile = None,
	options = {'py2exe': {'bundle_files': 1}}
)

setup(
	windows = ['nsupdate_tray.py', ],
	version = __version__,
	zipfile = None,
	options = {'py2exe': {'bundle_files': 1}}
)

'''
setup(
	name = 'dnsupdater',
	version = __version__,
	#modules = ['nsupdate.py'],
	scripts = ['nsupdate'],
	data_files = [
		('/etc', ['nsupdate.ini',]),
	]
)
'''