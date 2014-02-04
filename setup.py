from cx_Freeze import setup, Executable

from version import __version__


base = None
import sys
if sys.platform == 'win32': base = 'Win32GUI'

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
)
