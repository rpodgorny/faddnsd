from distutils.core import setup
import py2exe

from nsupdate import __version__ as version

setup(
	windows = ['nsupdate.py', ],
	version = version,
	zipfile = None,
	options = {'py2exe': {'bundle_files': 1}}
)
