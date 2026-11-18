#!/usr/bin/env python3
"""Linux native smoke test. Build GUI/CLI first; requires WebKitWebDriver, X11 and ffmpeg.
Uses temporary XDG data and anonymous official pages; never enters account credentials.
"""
import os,pty,select,sqlite3,subprocess,tempfile,time,json,threading,base64
from pathlib import Path
from http.server import BaseHTTPRequestHandler,HTTPServer
import requests
repo=Path(__file__).resolve().parents[2]
root=Path(tempfile.mkdtemp(prefix='dldr-play-native-'));data=root/'data';config=root/'config';profile=config/'dldr'
subprocess.run(['ffmpeg','-v','error','-f','lavfi','-i','anullsrc=r=44100:cl=mono','-t','3','-c:a','libmp3lame',str(root/'silence.mp3')],check=True)
# Initialize through the installed CLI and cancel before entering any credentials.
m,s=pty.openpty();child=subprocess.Popen([os.environ.get('DLDR_CLI_BINARY',str(repo/'target/release/dldr')),'--data-dir',str(profile),'login','--no-sync'],stdin=s,stdout=s,stderr=s,start_new_session=True)
seen=b''
for _ in range(100):
 ready,_,_=select.select([m],[],[],.1)
 if ready:seen+=os.read(m,65536)
 if b'DLsite login ID/email' in seen:break
else:raise AssertionError('Profile initialization did not reach prompt')
os.write(m,b'\x03');assert child.wait(10)==130;os.close(m);os.close(s)
db=sqlite3.connect(profile/'dldr.sqlite')
for account in ['qa-account-a','qa-account-b']:
 db.execute('INSERT INTO accounts(id,label,enabled) VALUES (?,?,1)',(account,account))
db.commit();db.close()
class Page(BaseHTTPRequestHandler):
 def do_GET(self):
  if self.path=='/silence.mp3':
   self.send_response(200);self.send_header('Content-Type','audio/mpeg');self.end_headers();self.wfile.write((root/'silence.mp3').read_bytes());return
  html=b'<!doctype html><html><body><h1>DLDR isolated native browser QA</h1><p>No real credentials or media.</p><audio id=audio muted controls src=/silence.mp3></audio></body></html>'
  self.send_response(200);self.send_header('Content-Type','text/html');self.end_headers();self.wfile.write(html)
 def log_message(self,*args):pass
server=HTTPServer(('127.0.0.1',1420),Page);threading.Thread(target=server.serve_forever,daemon=True).start()
env=os.environ.copy();env.update(XDG_DATA_HOME=str(data),XDG_CONFIG_HOME=str(config),XDG_CACHE_HOME=str(root/'cache'),TAURI_WEBVIEW_AUTOMATION='true',WEBKIT_DISABLE_DMABUF_RENDERER='1',GDK_BACKEND='x11')
log=open(root/'driver.log','w');driver=subprocess.Popen(['WebKitWebDriver','--port=9516'],env=env,stdout=log,stderr=log)
for _ in range(100):
 try:
  if requests.get('http://127.0.0.1:9516/status',timeout=1).ok:break
 except requests.RequestException:pass
 time.sleep(.1)
sid=None
try:
 r=requests.post('http://127.0.0.1:9516/session',json={'capabilities':{'alwaysMatch':{'webkitgtk:browserOptions':{'binary':str(repo/'target/debug/dldr-desktop')}}}},timeout=30)
 assert r.ok, r.text;obj=r.json();assert 'sessionId' in obj['value'],obj;sid=obj['value']['sessionId'];base='http://127.0.0.1:9516/session/'+sid
 def call(method,path,payload=None):
  v=requests.request(method,base+path,json=payload,timeout=30).json()['value']
  if isinstance(v,dict) and 'error' in v:raise AssertionError(v)
  return v
 def js(script,*args):return call('POST','/execute/sync',{'script':script,'args':list(args)})
 def wait(script):
  for _ in range(150):
   if js(script):return
   time.sleep(.1)
  raise AssertionError(script)
 wait('return !!window.__TAURI_INTERNALS__')
 # The main local view exercises real production command dispatch in a temporary profile.
 js("window.result=null;window.__TAURI_INTERNALS__.invoke('open_dlsite_play',{accountId:'qa-account-a',workId:null}).then(()=>window.result='ok',e=>window.result=String(e));")
 wait('return window.result!==null');assert js('return window.result')=='ok'
 js("window.result=null;window.__TAURI_INTERNALS__.invoke('open_dlsite_play',{accountId:'qa-account-a',workId:null}).then(()=>window.result='ok',e=>window.result=String(e));")
 wait('return window.result!==null');assert js('return window.result')=='ok'
 time.sleep(3)
 windows=subprocess.check_output(['xdotool','search','--name','DLsite Play.*qa-account-a']).decode().split()
 assert len(windows)==1,windows
 os.environ['GDK_BACKEND']='x11'
 import gi
 gi.require_version('Gtk','3.0');from gi.repository import Gtk,Gdk,GdkX11
 Gtk.init([])
 display=Gdk.Display.get_default();foreign=GdkX11.X11Window.foreign_new_for_display(display,int(windows[0]))
 geometry=foreign.get_geometry();picture=Gdk.pixbuf_get_from_window(foreign,0,0,geometry.width,geometry.height)
 picture.savev(str(root/'official-play.png'),'png',[],[])
 # Generated muted audio verifies the actual app WebKit MP3 decoder without playback of purchased media.
 js("window.mediaResult=null;const a=document.querySelector('audio');a.play().then(()=>window.mediaResult='ok',e=>window.mediaResult=e.name);")
 wait('return window.mediaResult!==null');assert js('return window.mediaResult')=='ok'
 wait("return document.querySelector('audio').currentTime>0.1")
 assert js("return document.querySelector('audio').duration")>2
 js("document.querySelector('audio').pause()")
 # Opening a second account uses a separate native browser context/profile.
 js("window.result=null;window.__TAURI_INTERNALS__.invoke('open_dlsite_play',{accountId:'qa-account-b',workId:null}).then(()=>window.result='ok',e=>window.result=String(e));")
 wait('return window.result!==null');assert js('return window.result')=='ok'
 assert len(list((profile/'play-browser').iterdir()))==2
 assert all((p.stat().st_mode & 0o777)==0o700 for p in (profile/'play-browser').iterdir())
 # Unknown work/account requests fail before navigation or download admission.
 for args in [{'accountId':'not-an-account','workId':None},{'accountId':'qa-account-a','workId':'../RJ1'}]:
  js("window.result=null;window.__TAURI_INTERNALS__.invoke('open_dlsite_play',arguments[0]).then(()=>window.result='ok',e=>window.result=String(e));",args)
  wait('return window.result!==null');assert js('return window.result')!='ok'
 db=sqlite3.connect(profile/'dldr.sqlite');assert db.execute('SELECT COUNT(*) FROM download_queue').fetchone()[0]==0;assert db.execute('SELECT COUNT(*) FROM accounts').fetchone()[0]==2;db.close()
 print(json.dumps({'passed':True,'root':str(root),'checks':['real main IPC','official WebView load','same-account window reuse','separate account profiles','0700 profiles','invalid work/account rejection','no queue/downloads','native MP3 decode/clock (muted)']}))
finally:
 if sid:
  try: requests.delete('http://127.0.0.1:9516/session/'+sid,timeout=10)
  except requests.RequestException:pass
 driver.terminate();driver.wait(10);server.shutdown();log.close()
