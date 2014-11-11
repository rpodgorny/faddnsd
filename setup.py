#!/usr/bin/python3

from setuptools import setup, find_packages

from version import __version__

setup(
	name = 'faddns',
	version = __version__,
	options = {
		'build_exe': {
			'compressed': True,
			'include_files': ['faddns.png', 'etc/faddnsc.conf']
		},
	},
	scripts = ['faddnsc', 'faddnsd', 'faddns_update_zone'],
	packages = find_packages(),
)
