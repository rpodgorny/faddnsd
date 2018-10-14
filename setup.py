#!/usr/bin/python3

from setuptools import setup, find_packages

from version import __version__

setup(
	name = 'faddns',
	version = __version__,
	options = {
		'build_exe': {
			'compressed': True,
			'include_files': ['etc/faddnsc.conf', ]
		},
	},
	scripts = ['faddnsd', ],
	packages = find_packages(),
)
