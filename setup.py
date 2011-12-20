from distutils.core import setup
import py2exe

setup(
	console = ['nsupdate.py', ],
	#version = __import__(srcs[0].split('.')[0]).__version__,
	zipfile = None,
	options = {'py2exe': {'bundle_files': 1}}
)
