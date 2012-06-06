from nsupdate import __version__

import sys
import wx
import xmlrpclib

import log
sys.excepthook = log.log_exception
log.filename = 'nsupdate_tray.log'

_exit = False

class Tray(wx.TaskBarIcon):
	def CreatePopupMenu(self):
		menu = wx.Menu()
		menu.Append(123, 'exit')
		self.Bind(wx.EVT_MENU, self.on_exit, id=123)
		return menu
	#enddef

	def on_exit(self, e):
		log.log('clicked exit')

		try:
			_s.exit()
		except:
			log.log('failed to call remote exit')
		#endtry

		wx.GetApp().ExitMainLoop()
	#enddef
#endclass

def main():
	log.log('*' * 40)
	log.log('starting nsupdate tray v%s' % __version__)

	global _s
	_s = xmlrpclib.ServerProxy('http://localhost:8889')

	app = wx.App(0)

	tb = Tray()

	log.log('loading icon')
	icon = wx.Icon('nsupdate.png', wx.BITMAP_TYPE_PNG)
	tb.SetIcon(icon, 'nsupdate')

	log.log('starting MainLoop')
	app.MainLoop()
	log.log('exited MainLoop')
#enddef

if __name__ == '__main__':
	main()
#endif