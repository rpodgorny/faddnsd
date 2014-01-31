import sys
from version import __version__


if sys.platform == 'win32':
	from cx_Freeze import setup, Executable

	base = 'Win32GUI'

	executables = [
		Executable(
			script='faddnsc',
			appendScriptToExe=True,
			appendScriptToLibrary=False,
			compress=True,
		),
		Executable(
			script='faddnsc_gui.py',
			appendScriptToExe=True,
			appendScriptToLibrary=False,
			compress=True,
			base=base
		)
	]
else:
	from setuptools import setup

	base = None
#endif


setup(
	name = 'faddns',
	version = __version__,
	options = {
		'build_exe': {
			'includes': ['re', ],
			'create_shared_zip': False,
			'compressed': True,
			'include_msvcr': True
		},
	},
	scripts = ['faddnsc']
)
