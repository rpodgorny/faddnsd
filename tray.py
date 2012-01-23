import wx
import thread

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
		global exit
		_exit = True
		print 'exit'
	#enddef
#endclass

def run_app():
	app = wx.App(0)

	icon = wx.Icon('icon.jpg', wx.BITMAP_TYPE_JPEG)
	global tb
	tb = Tray()
	tb.SetIcon(icon, 'nsupdate')

	app.MainLoop()
#enddef

def run():
	thread.start_new_thread(run_app, ())
#enddef

def exit():
	tb.Destroy()
#enddef