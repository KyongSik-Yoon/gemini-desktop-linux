package cua

import (
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"image"
	"image/jpeg"
	_ "image/png"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"sync"
	"time"
)

// ExecutionResult는 ui_control 실행 결과와 캡처된 스크린샷, UI 트리 상태를 담는다.
type ExecutionResult struct {
	Stdout         string
	Stderr         string
	ExitCode       int
	ScreenshotJPEG []byte
	UIState        string
}

// RunUIControl은 macOS Gemini.app의 CUAScriptRuntime과 호환되는 Python 환경을 구성해
// Wayland(Hyprland: hyprctl + grim + wtype) 및 X11(xdotool + import)에서 GUI 자동화를 수행한다.
func RunUIControl(ctx context.Context, code string, captureScreenshot bool) (*ExecutionResult, error) {
	tmpDir, err := os.MkdirTemp("", "gemini-cua-*")
	if err != nil {
		return nil, err
	}
	defer os.RemoveAll(tmpDir)

	shotPath := filepath.Join(tmpDir, "cua_screenshot.png")
	prelude := buildPythonPrelude(shotPath)
	fullScript := prelude + "\n" + code

	scriptFile := filepath.Join(tmpDir, "run_cua.py")
	if err := os.WriteFile(scriptFile, []byte(fullScript), 0o600); err != nil {
		return nil, err
	}

	runCtx, cancel := context.WithTimeout(ctx, 60*time.Second)
	defer cancel()

	cmd := exec.CommandContext(runCtx, "python3", scriptFile)
	var stdout, stderr bytes.Buffer
	cmd.Stdout = &stdout
	cmd.Stderr = &stderr

	err = cmd.Run()
	exitCode := 0
	if err != nil {
		if ee, ok := err.(*exec.ExitError); ok {
			exitCode = ee.ExitCode()
		} else {
			exitCode = 1
		}
	}

	res := &ExecutionResult{
		Stdout:   stdout.String(),
		Stderr:   stderr.String(),
		ExitCode: exitCode,
		UIState:  CollectUIState(ctx),
	}

	if captureScreenshot {
		if _, err := os.Stat(shotPath); err != nil {
			_ = takeScreenshotPNG(ctx, shotPath)
		}
		if pngBytes, err := os.ReadFile(shotPath); err == nil && len(pngBytes) > 0 {
			if jpgBytes, err := pngToJPEG(pngBytes); err == nil {
				res.ScreenshotJPEG = jpgBytes
			} else {
				res.ScreenshotJPEG = pngBytes
			}
		}
	}

	return res, nil
}

// CollectUIState는 Hyprland 창 목록·포그라운드 창 정보와 AT-SPI2 접근성 트리를 병렬 조회해 반환한다.
func CollectUIState(ctx context.Context) string {
	cctx, cancel := context.WithTimeout(ctx, 3*time.Second)
	defer cancel()

	var (
		wg        sync.WaitGroup
		activeOut []byte
		clientOut []byte
		a11yTree  string
	)
	wg.Add(3)
	go func() {
		defer wg.Done()
		if out, err := exec.CommandContext(cctx, "hyprctl", "activewindow", "-j").Output(); err == nil {
			activeOut = out
		}
	}()
	go func() {
		defer wg.Done()
		if out, err := exec.CommandContext(cctx, "hyprctl", "clients", "-j").Output(); err == nil {
			clientOut = out
		}
	}()
	go func() {
		defer wg.Done()
		a11yTree = collectATSPITree(cctx)
	}()
	wg.Wait()

	var sb strings.Builder
	if len(activeOut) > 0 {
		var win map[string]any
		if json.Unmarshal(activeOut, &win) == nil {
			sb.WriteString(fmt.Sprintf("ActiveWindow: class=%v title=%q at=%v size=%v workspace=%v\n",
				win["class"], win["title"], win["at"], win["size"], win["workspace"]))
		}
	}
	if len(clientOut) > 0 {
		var clients []map[string]any
		if json.Unmarshal(clientOut, &clients) == nil {
			sb.WriteString("Windows:\n")
			for i, c := range clients {
				if i >= 25 {
					break
				}
				sb.WriteString(fmt.Sprintf("  - [%v] %q at=%v size=%v\n",
					c["class"], c["title"], c["at"], c["size"]))
			}
		}
	}
	if a11yTree != "" {
		sb.WriteString("AccessibilityTree:\n")
		sb.WriteString(a11yTree)
		sb.WriteByte('\n')
	}
	return strings.TrimSpace(sb.String())
}

// collectATSPITree는 pyatspi가 설치된 환경에서 포커스된 앱의 인터랙티브 UI 요소 요약을 수집한다.
func collectATSPITree(ctx context.Context) string {
	actx, cancel := context.WithTimeout(ctx, 1500*time.Millisecond)
	defer cancel()
	const script = `import sys
try:
    import pyatspi
except Exception:
    sys.exit(0)
try:
    desktop = pyatspi.Registry.getDesktop(0)
except Exception:
    sys.exit(0)
lines = []
INTERACTIVE = {
    "push button", "toggle button", "check box", "radio button",
    "text", "entry", "password text", "menu item", "page tab",
    "combo box", "link", "list item", "spin button", "slider"
}
def walk(acc, depth=0):
    if len(lines) >= 40 or depth > 6 or acc is None:
        return
    try:
        role = acc.getRoleName()
        name = (acc.name or "").strip()
    except Exception:
        return
    if role in INTERACTIVE and name:
        rect = ""
        try:
            comp = acc.queryComponent()
            ext = comp.getExtents(pyatspi.DESKTOP_COORDS)
            if ext.width > 0 and ext.height > 0:
                rect = f" rect=({ext.x},{ext.y},{ext.width},{ext.height})"
        except Exception:
            pass
        lines.append(f"  - [{role}] {name[:60]!r}{rect}")
    try:
        n = min(acc.childCount, 40)
        for i in range(n):
            walk(acc.getChildAtIndex(i), depth + 1)
    except Exception:
        pass
for app in desktop:
    try:
        for i in range(min(app.childCount, 8)):
            win = app.getChildAtIndex(i)
            st = win.getState()
            if st.contains(pyatspi.STATE_ACTIVE):
                walk(win, 0)
                break
    except Exception:
        continue
    if lines:
        break
print("\n".join(lines))
`
	out, err := exec.CommandContext(actx, "python3", "-c", script).Output()
	if err != nil {
		return ""
	}
	return strings.TrimSpace(string(out))
}

func takeScreenshotPNG(ctx context.Context, outPath string) error {
	cctx, cancel := context.WithTimeout(ctx, 5*time.Second)
	defer cancel()
	if _, err := exec.LookPath("grim"); err == nil {
		return exec.CommandContext(cctx, "grim", outPath).Run()
	}
	if _, err := exec.LookPath("import"); err == nil {
		return exec.CommandContext(cctx, "import", "-window", "root", outPath).Run()
	}
	return fmt.Errorf("스크린샷 도구를 찾을 수 없습니다 (grim 또는 imagemagick 필요)")
}

func pngToJPEG(pngData []byte) ([]byte, error) {
	img, _, err := image.Decode(bytes.NewReader(pngData))
	if err != nil {
		return nil, err
	}
	var out bytes.Buffer
	if err := jpeg.Encode(&out, img, &jpeg.Options{Quality: 82}); err != nil {
		return nil, err
	}
	return out.Bytes(), nil
}

func buildPythonPrelude(shotPath string) string {
	return fmt.Sprintf(`import os, subprocess, time, shutil, json, socket, struct

_SHOT_PATH = %q

def _run(cmd):
    return subprocess.run(cmd, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)

def _wl_str(s):
    b = s.encode("utf-8") + b"\x00"
    pad = (4 - (len(b) %% 4)) %% 4
    return struct.pack("<I", len(b)) + b + (b"\x00" * pad)

def _wl_msg(oid, op, payload=b""):
    size = 8 + len(payload)
    return struct.pack("<II", oid, (size << 16) | op) + payload

class _WlrVirtualPointer:
    def __init__(self):
        runtime = os.environ.get("XDG_RUNTIME_DIR", f"/run/user/{os.getuid()}")
        disp = os.environ.get("WAYLAND_DISPLAY", "")
        if not disp:
            raise RuntimeError("WAYLAND_DISPLAY not set")
        self.sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        self.sock.settimeout(3.0)
        self.sock.connect(os.path.join(runtime, disp))
        self.next_id = 2
        reg_id = self._alloc_id()
        cb_id = self._alloc_id()
        self.sock.sendall(
            _wl_msg(1, 1, struct.pack("<I", reg_id))
            + _wl_msg(1, 0, struct.pack("<I", cb_id))
        )
        self.globals = {}
        self._read_until_callback(reg_id, cb_id)
        if "zwlr_virtual_pointer_manager_v1" not in self.globals:
            self.sock.close()
            raise RuntimeError("zwlr_virtual_pointer_manager_v1 not supported")
        mgr_name, mgr_ver = self.globals["zwlr_virtual_pointer_manager_v1"]
        self.mgr_id = self._alloc_id()
        bind_payload = (
            struct.pack("<I", mgr_name)
            + _wl_str("zwlr_virtual_pointer_manager_v1")
            + struct.pack("<II", min(mgr_ver, 2), self.mgr_id)
        )
        self.ptr_id = self._alloc_id()
        create_payload = struct.pack("<II", 0, self.ptr_id)
        self.sock.sendall(
            _wl_msg(reg_id, 0, bind_payload) + _wl_msg(self.mgr_id, 0, create_payload)
        )
        self.sync()
        self._last_x = 0
        self._last_y = 0

    def _alloc_id(self):
        i = self.next_id
        self.next_id += 1
        return i

    def _read_until_callback(self, reg_id, cb_id):
        buf = b""
        while True:
            chunk = self.sock.recv(4096)
            if not chunk:
                break
            buf += chunk
            while len(buf) >= 8:
                oid, sz_op = struct.unpack("<II", buf[:8])
                sz, op = sz_op >> 16, sz_op & 0xFFFF
                if len(buf) < sz:
                    break
                payload = buf[8:sz]
                buf = buf[sz:]
                if oid == reg_id and op == 0:
                    name, slen = struct.unpack("<II", payload[:8])
                    iface = payload[8 : 8 + slen - 1].decode("utf-8", "replace")
                    ver_off = 8 + ((slen + 3) & ~3)
                    ver = struct.unpack("<I", payload[ver_off : ver_off + 4])[0]
                    self.globals[iface] = (name, ver)
                elif oid == cb_id and op == 0:
                    return

    def sync(self):
        cb_id = self._alloc_id()
        self.sock.sendall(_wl_msg(1, 0, struct.pack("<I", cb_id)))
        self._read_until_callback(-1, cb_id)

    def _ts(self):
        return int(time.monotonic() * 1000) & 0xFFFFFFFF

    def _bounds(self):
        if shutil.which("hyprctl"):
            r = _run(["hyprctl", "monitors", "-j"])
            if r.returncode == 0 and r.stdout.strip():
                mons = json.loads(r.stdout)
                if mons:
                    min_x = min(m["x"] for m in mons)
                    min_y = min(m["y"] for m in mons)
                    max_x = max(m["x"] + int(round(m["width"] / m.get("scale", 1.0))) for m in mons)
                    max_y = max(m["y"] + int(round(m["height"] / m.get("scale", 1.0))) for m in mons)
                    return min_x, min_y, max(1, max_x - min_x), max(1, max_y - min_y)
        return 0, 0, 1920, 1080

    def move_abs(self, x, y):
        min_x, min_y, w, h = self._bounds()
        lx = max(0, min(w - 1, int(x) - min_x))
        ly = max(0, min(h - 1, int(y) - min_y))
        self._last_x = int(x)
        self._last_y = int(y)
        self.sock.sendall(
            _wl_msg(self.ptr_id, 1, struct.pack("<IIIII", self._ts(), lx, ly, w, h))
            + _wl_msg(self.ptr_id, 4)
        )
        self.sync()

    def button(self, code, pressed):
        self.sock.sendall(
            _wl_msg(self.ptr_id, 2, struct.pack("<III", self._ts(), code, 1 if pressed else 0))
            + _wl_msg(self.ptr_id, 4)
        )
        self.sync()

    def scroll(self, dx, dy):
        for axis, delta in ((0, dy), (1, dx)):
            if not delta:
                continue
            steps = max(1, min(20, abs(int(delta))))
            step_px = 15 if delta > 0 else -15
            disc = 1 if delta > 0 else -1
            val_fixed = int(step_px * 256)
            for _ in range(steps):
                self.sock.sendall(
                    _wl_msg(self.ptr_id, 5, struct.pack("<I", 0))
                    + _wl_msg(self.ptr_id, 7, struct.pack("<IIii", self._ts(), axis, val_fixed, disc))
                    + _wl_msg(self.ptr_id, 4)
                )
                self.sync()
                time.sleep(0.03)

_WLR_PTR = None
_WLR_INIT_FAILED = False

def _get_wlr_ptr():
    global _WLR_PTR, _WLR_INIT_FAILED
    if _WLR_PTR is not None:
        return _WLR_PTR
    if _WLR_INIT_FAILED:
        return None
    try:
        _WLR_PTR = _WlrVirtualPointer()
        return _WLR_PTR
    except Exception:
        _WLR_INIT_FAILED = True
        return None

_BTN_LINUX = {"left": 272, "right": 273, "middle": 274}
_MOD_ALIAS = {
    "cmd": "super", "command": "super", "meta": "super", "win": "super", "super": "super", "logo": "super",
    "ctrl": "ctrl", "control": "ctrl",
    "alt": "alt", "opt": "alt", "option": "alt",
    "shift": "shift",
}
_KEY_ALIAS = {
    "enter": "Return", "return": "Return", "esc": "Escape", "escape": "Escape",
    "tab": "Tab", "space": "space", "backspace": "BackSpace", "delete": "Delete",
    "up": "Up", "down": "Down", "left": "Left", "right": "Right",
    "home": "Home", "end": "End", "pageup": "Page_Up", "pagedown": "Page_Down",
}

def _norm_key(k):
    s = str(k).strip()
    return _KEY_ALIAS.get(s.lower(), s)

def move_to(x, y):
    vp = _get_wlr_ptr()
    if vp is not None:
        vp.move_abs(x, y)
        return
    if shutil.which("xdotool"):
        _run(["xdotool", "mousemove", str(int(x)), str(int(y))])
    elif shutil.which("ydotool"):
        _run(["ydotool", "mousemove", "--absolute", "-x", str(int(x)), "-y", str(int(y))])

def click(x=None, y=None, button="left"):
    b_name = str(button).lower()
    vp = _get_wlr_ptr()
    if vp is not None:
        if x is not None and y is not None:
            vp.move_abs(x, y)
            time.sleep(0.05)
        code = _BTN_LINUX.get(b_name, 272)
        vp.button(code, True)
        time.sleep(0.04)
        vp.button(code, False)
        return
    if x is not None and y is not None:
        move_to(x, y)
        time.sleep(0.05)
    btn_map = {"left": "1", "middle": "2", "right": "3"}
    b = btn_map.get(b_name, "1")
    if shutil.which("ydotool"):
        code_map = {"1": "0xC0", "2": "0xC2", "3": "0xC1"}
        _run(["ydotool", "click", code_map.get(b, "0xC0")])
    elif shutil.which("xdotool"):
        _run(["xdotool", "click", b])

def double_click(x=None, y=None):
    click(x, y, "left")
    time.sleep(0.08)
    click(None, None, "left")

def right_click(x=None, y=None):
    click(x, y, "right")

def drag_to(x, y, button="left", start_x=None, start_y=None):
    b_name = str(button).lower()
    vp = _get_wlr_ptr()
    if vp is not None:
        if start_x is not None and start_y is not None:
            vp.move_abs(start_x, start_y)
            time.sleep(0.04)
        sx, sy = vp._last_x, vp._last_y
        code = _BTN_LINUX.get(b_name, 272)
        vp.button(code, True)
        time.sleep(0.04)
        steps = 6
        for i in range(1, steps + 1):
            nx = sx + (int(x) - sx) * i // steps
            ny = sy + (int(y) - sy) * i // steps
            vp.move_abs(nx, ny)
            time.sleep(0.02)
        vp.button(code, False)
        return
    btn_map = {"left": "1", "middle": "2", "right": "3"}
    b = btn_map.get(b_name, "1")
    if start_x is not None and start_y is not None:
        move_to(start_x, start_y)
        time.sleep(0.04)
    if shutil.which("ydotool"):
        down_map = {"1": "0x40", "2": "0x42", "3": "0x41"}
        up_map = {"1": "0x80", "2": "0x82", "3": "0x81"}
        _run(["ydotool", "click", down_map.get(b, "0x40")])
        time.sleep(0.05)
        move_to(x, y)
        time.sleep(0.05)
        _run(["ydotool", "click", up_map.get(b, "0x80")])
    elif shutil.which("xdotool"):
        _run(["xdotool", "mousedown", b, "mousemove", "--sync", str(int(x)), str(int(y)), "mouseup", b])
    else:
        move_to(x, y)

def type_text(text):
    if shutil.which("wtype"):
        _run(["wtype", "--", str(text)])
    elif shutil.which("xdotool"):
        _run(["xdotool", "type", "--clearmodifiers", "--", str(text)])

def press_key(key):
    nk = _norm_key(key)
    if shutil.which("wtype"):
        _run(["wtype", "-k", nk])
    elif shutil.which("xdotool"):
        _run(["xdotool", "key", nk])

def hotkey(*keys):
    if not keys:
        return
    mods = [_MOD_ALIAS.get(str(k).strip().lower(), str(k).strip().lower()) for k in keys[:-1]]
    last = _norm_key(keys[-1])
    if shutil.which("wtype"):
        args = ["wtype"]
        for m in mods:
            args += ["-M", m]
        args += ["-k", last]
        for m in reversed(mods):
            args += ["-m", m]
        _run(args)
    elif shutil.which("xdotool"):
        _run(["xdotool", "key", "+".join(mods + [last])])

def scroll(dx=0, dy=0, x=None, y=None):
    vp = _get_wlr_ptr()
    if vp is not None:
        if x is not None and y is not None:
            vp.move_abs(x, y)
            time.sleep(0.04)
        vp.scroll(dx, dy)
        return
    if x is not None and y is not None:
        move_to(x, y)
    if shutil.which("xdotool") and dy != 0:
        btn = "4" if dy > 0 else "5"
        _run(["xdotool", "click", "--repeat", str(abs(int(dy))), btn])

def wait(seconds=1.0):
    time.sleep(float(seconds))

def open_app(name):
    s = str(name).strip()
    if not s:
        return
    if shutil.which(s):
        subprocess.Popen([s], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        return
    if shutil.which("gtk-launch") and " " not in s and "/" not in s:
        r = _run(["gtk-launch", s])
        if r.returncode == 0:
            return
    subprocess.Popen(["sh", "-c", s], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)

def open_application(name):
    return open_app(name)

def screenshot():
    if shutil.which("grim"):
        _run(["grim", _SHOT_PATH])
    elif shutil.which("import"):
        _run(["import", "-window", "root", _SHOT_PATH])
    return _SHOT_PATH

def observe_window():
    if shutil.which("hyprctl"):
        r = _run(["hyprctl", "activewindow", "-j"])
        return r.stdout
    return ""

read_screen = observe_window
read_som = observe_window
handleClick = click
handleType = type_text
handleMove = move_to
handleWait = wait
handleTakeScreenshot = screenshot
`, shotPath)
}
