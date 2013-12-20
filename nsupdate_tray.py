#!/usr/bin/python3

from version import __version__

import sys
from PySide.QtCore import *
from PySide.QtGui import *
import logging


class MyTray(QSystemTrayIcon):
	def __init__(self, app):
		icon = QIcon('nsupdate.png')
		super().__init__(icon)

		self.app = app

		self.setToolTip('nsupdate v%s' % __version__)

		menu = QMenu()
		menu.addAction('Force refresh', self.on_refresh)
		menu.addAction('Exit', self.on_exit)
		self.setContextMenu(menu)

		self.show()
	#enddef

	def on_refresh(self):
		pass
	#enddef

	def on_exit(self):
		self.app.quit()
	#enddef
#endclass


def logging_setup():
	logging.basicConfig(level='DEBUG')
#enddef


def main():
	logging_setup()

	logging.info('*' * 40)
	logging.info('starting nsupdate tray v%s' % __version__)

	app = QApplication(sys.argv[1:])

	tray = MyTray(app)

	app.exec_()

	logging.info('done')
#enddef


if __name__ == '__main__':
	main()
#endif
