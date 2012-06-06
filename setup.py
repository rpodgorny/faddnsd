from distutils.core import setup
import py2exe

from nsupdate import __version__

setup(
	console = ['nsupdate.py', ],
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
