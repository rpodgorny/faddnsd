import sys
from cx_Freeze import setup, Executable

from faddns.version import __version__


base = 'Win32GUI'

executables = [
	Executable(
		script='faddnsc',
		appendScriptToExe=True,
		appendScriptToLibrary=False,
		compress=True,
	),
	#Executable(
	#	script='faddnsc_gui',
	#	appendScriptToExe=True,
	#	appendScriptToLibrary=False,
	#	compress=True,
	#	base=base
	#),
]

setup(
	name = 'faddns',
	version = __version__,
	options = {
		'build_exe': {
			'includes': ['re', ],
			'create_shared_zip': False,
			'compressed': True,
			'include_msvcr': True,
			'include_files': ['faddns.png', 'faddnsc.ini']
		},
	},
	executables = executables,
)
