import wx
import thread

import logging
logger = logging.getLogger('tray')

_exit = False
tb = None

class Tray(wx.TaskBarIcon):
	def CreatePopupMenu(self):
		menu = wx.Menu()
		menu.Append(123, 'exit')
		self.Bind(wx.EVT_MENU, self.on_exit, id=123)
		return menu
	#enddef

	def on_exit(self, e):
		global _exit
		_exit = True
		logger.debug('clicked exit')
	#enddef
#endclass

def run_app():
	app = wx.App(0)

	global tb
	tb = Tray()
	
	logger.debug('loading icon')
	icon = wx.Icon('icon.jpg', wx.BITMAP_TYPE_JPEG)
	tb.SetIcon(icon, 'nsupdate')

	logger.info('starting MainLoop')
	app.MainLoop()
#enddef

def run():
	logger.debug('run')

	thread.start_new_thread(run_app, ())
#enddef

def exit():
	logger.debug('exit')

	tb.Destroy()
#enddef
