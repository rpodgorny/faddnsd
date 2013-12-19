#!/usr/bin/python3

from version import __version__

import sys
from PySide.QtCore import *
from PySide.QtGui import *
import logging


def logging_setup():
	logging.basicConfig(level='DEBUG')
#enddef


def main():
	logging_setup()

	logging.info('*' * 40)
	logging.info('starting nsupdate tray v%s' % __version__)

	app = QApplication(sys.argv[1:])

	icon = QIcon('nsupdate.png')
	tray = QSystemTrayIcon(icon)

	app.exec_()

	logging.info('done')
#enddef


if __name__ == '__main__':
	main()
#endif
