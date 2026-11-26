package config

import (
	"crypto/rand"
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"sync"
)

const (
	MaxBootstrapFileSize = 32768 // 0x8000 bytes (macOS Gemini.app 상한과 동일)
	DefaultOSVersion     = "15.4.0"
	DefaultOSBuild       = "24E248"
	DefaultAppVersion    = "1.2026.0820001"
)

// MCPServerConfig는 외부 또는 내장 MCP 서버 연결 설정을 담는다.
type MCPServerConfig struct {
	ServerID    string            `json:"server_id"`
	DisplayName string            `json:"display_name"`
	Instruction string            `json:"instruction,omitempty"`
	Command     string            `json:"command,omitempty"`
	Args        []string          `json:"args,omitempty"`
	Env         map[string]string `json:"env,omitempty"`
	Enabled     bool              `json:"enabled"`
}

// MCPBridgeConfig는 `gemini-remote mcp-serve` MCP 위임 서버의 기본 실행 설정을 담는다.
type MCPBridgeConfig struct {
	DefaultModel         string `json:"default_model,omitempty"`
	DefaultThinkingLevel string `json:"default_thinking_level,omitempty"` // "", "auto", "low", "medium", "high"
	DefaultDeviceID      string `json:"default_device_id,omitempty"`      // 비어 있으면 로컬 PC(DeviceID)
	DefaultAgentTask     *bool  `json:"default_agent_task,omitempty"`     // 미지정 시 true
	DefaultTemporary     bool   `json:"default_temporary,omitempty"`
}

// EffectiveAgentTask는 기본 agent_task 활성화 여부를 반환한다(미지정 시 true).
func (b MCPBridgeConfig) EffectiveAgentTask() bool {
	if b.DefaultAgentTask == nil {
		return true
	}
	return *b.DefaultAgentTask
}

// NormalizeThinkingLevel은 사고 수준 문자열을 정규화하고 프로토콜 enum 값으로 변환한다.
func NormalizeThinkingLevel(raw string) (string, int32, error) {
	switch strings.ToLower(strings.TrimSpace(raw)) {
	case "", "auto", "0":
		return "auto", 0, nil
	case "low", "1":
		return "low", 1, nil
	case "medium", "med", "4":
		return "medium", 4, nil
	case "high", "2":
		return "high", 2, nil
	default:
		return "", 0, fmt.Errorf("지원하지 않는 thinking level: %q (auto, low, medium, high 중 선택)", raw)
	}
}

// Config는 gemini-remote-linux 데몬의 영구 설정을 담는다.
type Config struct {
	mu sync.RWMutex `json:"-"`

	DeviceID          string            `json:"device_id"`
	DeviceName        string            `json:"device_name"`
	OSType            string            `json:"os_type"`
	OSVersion         string            `json:"os_version"`
	OSBuild           string            `json:"os_build"`
	AppVersion        string            `json:"app_version"`
	Locale            string            `json:"locale"`
	TimeZone          string            `json:"time_zone"`
	ConnectedFolders  []string          `json:"connected_folders"`
	MCPServers        []MCPServerConfig `json:"mcp_servers"`
	MCPBridge         MCPBridgeConfig   `json:"mcp_bridge,omitempty"`
	RemoteEnabled     *bool             `json:"remote_enabled,omitempty"`
	ObsidianVaultDir  string            `json:"obsidian_vault_dir,omitempty"`
	OAuthClientID     string            `json:"oauth_client_id,omitempty"`
	OAuthClientSecret string            `json:"oauth_client_secret,omitempty"`
}

// GetMCPBridge는 MCP 브릿지 기본 설정 복사본을 스레드 안전하게 반환한다.
func (c *Config) GetMCPBridge() MCPBridgeConfig {
	if c == nil {
		return MCPBridgeConfig{}
	}
	c.mu.RLock()
	defer c.mu.RUnlock()
	cp := c.MCPBridge
	if c.MCPBridge.DefaultAgentTask != nil {
		v := *c.MCPBridge.DefaultAgentTask
		cp.DefaultAgentTask = &v
	}
	return cp
}

// SetMCPBridge는 MCP 브릿지 기본 설정을 스레드 안전하게 갱신한다.
func (c *Config) SetMCPBridge(b MCPBridgeConfig) {
	if c == nil {
		return
	}
	c.mu.Lock()
	cp := b
	if b.DefaultAgentTask != nil {
		v := *b.DefaultAgentTask
		cp.DefaultAgentTask = &v
	}
	c.MCPBridge = cp
	c.mu.Unlock()
}

// IsRemoteEnabled는 원격 제어 수신이 활성화되어 있는지 반환한다(미설정 시 기본 true).
func (c *Config) IsRemoteEnabled() bool {
	if c == nil {
		return true
	}
	c.mu.RLock()
	defer c.mu.RUnlock()
	if c.RemoteEnabled == nil {
		return true
	}
	return *c.RemoteEnabled
}

// SetRemoteEnabled는 원격 제어 수신 활성화 여부를 설정한다.
func (c *Config) SetRemoteEnabled(enabled bool) {
	if c == nil {
		return
	}
	c.mu.Lock()
	v := enabled
	c.RemoteEnabled = &v
	c.mu.Unlock()
}

// CloneConnectedFolders는 연결된 폴더 목록의 복사본을 스레드 안전하게 반환한다.
func (c *Config) CloneConnectedFolders() []string {
	if c == nil {
		return nil
	}
	c.mu.RLock()
	defer c.mu.RUnlock()
	return append([]string(nil), c.ConnectedFolders...)
}

// FirstConnectedFolder는 첫 번째 연결 폴더를 스레드 안전하게 반환한다.
func (c *Config) FirstConnectedFolder() string {
	if c == nil {
		return ""
	}
	c.mu.RLock()
	defer c.mu.RUnlock()
	if len(c.ConnectedFolders) == 0 {
		return ""
	}
	return c.ConnectedFolders[0]
}

// AddConnectedFolder는 중복을 제외하고 연결 폴더를 추가한다(추가 시 true 반환).
func (c *Config) AddConnectedFolder(clean string) bool {
	if c == nil || clean == "" {
		return false
	}
	c.mu.Lock()
	defer c.mu.Unlock()
	for _, cur := range c.ConnectedFolders {
		if filepath.Clean(cur) == clean {
			return false
		}
	}
	c.ConnectedFolders = append(c.ConnectedFolders, clean)
	return true
}

// RemoveConnectedFolder는 연결 폴더 목록에서 일치하는 항목을 제거한다.
func (c *Config) RemoveConnectedFolder(clean, raw string) bool {
	if c == nil {
		return false
	}
	c.mu.Lock()
	defer c.mu.Unlock()
	next := make([]string, 0, len(c.ConnectedFolders))
	removed := false
	for _, cur := range c.ConnectedFolders {
		if filepath.Clean(cur) == clean || cur == raw {
			removed = true
			continue
		}
		next = append(next, cur)
	}
	if !removed {
		return false
	}
	c.ConnectedFolders = next
	return true
}

// CloneMCPServers는 MCP 서버 설정 목록의 복사본을 스레드 안전하게 반환한다.
func (c *Config) CloneMCPServers() []MCPServerConfig {
	if c == nil {
		return nil
	}
	c.mu.RLock()
	defer c.mu.RUnlock()
	out := make([]MCPServerConfig, len(c.MCPServers))
	for i, s := range c.MCPServers {
		cp := s
		if len(s.Args) > 0 {
			cp.Args = append([]string(nil), s.Args...)
		}
		if len(s.Env) > 0 {
			cp.Env = make(map[string]string, len(s.Env))
			for k, v := range s.Env {
				cp.Env[k] = v
			}
		}
		out[i] = cp
	}
	return out
}

// GetLocale은 설정된 로케일을 스레드 안전하게 반환한다.
func (c *Config) GetLocale() string {
	if c == nil {
		return "ko-KR"
	}
	c.mu.RLock()
	v := c.Locale
	c.mu.RUnlock()
	if strings.TrimSpace(v) == "" {
		return "ko-KR"
	}
	return v
}

// GetTimeZone은 설정된 시간대를 스레드 안전하게 반환한다.
func (c *Config) GetTimeZone() string {
	if c == nil {
		return "Asia/Seoul"
	}
	c.mu.RLock()
	v := c.TimeZone
	c.mu.RUnlock()
	if strings.TrimSpace(v) == "" {
		return "Asia/Seoul"
	}
	return v
}

// GetAppVersion은 설정된 앱 버전을 스레드 안전하게 반환한다.
func (c *Config) GetAppVersion() string {
	if c == nil {
		return DefaultAppVersion
	}
	c.mu.RLock()
	v := c.AppVersion
	c.mu.RUnlock()
	if strings.TrimSpace(v) == "" {
		return DefaultAppVersion
	}
	return v
}

// GetObsidianVaultDir는 설정된 Obsidian 볼트 경로를 스레드 안전하게 반환한다.
func (c *Config) GetObsidianVaultDir() string {
	if c == nil {
		return ""
	}
	c.mu.RLock()
	defer c.mu.RUnlock()
	return c.ObsidianVaultDir
}

// ConfigDir는 ~/.config/gemini-remote 경로를 반환한다.
func ConfigDir() (string, error) {
	base := os.Getenv("XDG_CONFIG_HOME")
	if base == "" {
		home, err := os.UserHomeDir()
		if err != nil {
			return "", err
		}
		base = filepath.Join(home, ".config")
	}
	dir := filepath.Join(base, "gemini-remote")
	if err := os.MkdirAll(dir, 0o700); err != nil {
		return "", err
	}
	return dir, nil
}

// ConfigPath는 설정 파일 경로를 반환한다.
func ConfigPath() (string, error) {
	dir, err := ConfigDir()
	if err != nil {
		return "", err
	}
	return filepath.Join(dir, "config.json"), nil
}

// TokenPath는 OAuth 토큰 저장 경로를 반환한다.
func TokenPath() (string, error) {
	dir, err := ConfigDir()
	if err != nil {
		return "", err
	}
	return filepath.Join(dir, "token.json"), nil
}

// SocketPath는 로컬 IPC Unix 도메인 소켓 경로를 반환한다.
func SocketPath() (string, error) {
	if runtimeDir := os.Getenv("XDG_RUNTIME_DIR"); runtimeDir != "" {
		if st, err := os.Stat(runtimeDir); err == nil && st.IsDir() {
			return filepath.Join(runtimeDir, "gemini-remote.sock"), nil
		}
	}
	dir, err := ConfigDir()
	if err != nil {
		return "", err
	}
	return filepath.Join(dir, "daemon.sock"), nil
}

// LoadOrInit은 설정 파일을 읽거나 기본값으로 초기화해 저장한다.
func LoadOrInit() (*Config, error) {
	p, err := ConfigPath()
	if err != nil {
		return nil, err
	}
	if data, err := os.ReadFile(p); err == nil {
		var cfg Config
		if err := json.Unmarshal(data, &cfg); err == nil {
			cfg.applyDefaults()
			return &cfg, nil
		}
	}
	cfg := &Config{}
	cfg.applyDefaults()
	if err := cfg.Save(); err != nil {
		return nil, err
	}
	return cfg, nil
}

func (c *Config) applyDefaults() {
	if strings.TrimSpace(c.DeviceID) == "" {
		c.DeviceID = newUUIDv4()
	}
	c.DeviceID = strings.ToUpper(strings.TrimSpace(c.DeviceID))
	if strings.TrimSpace(c.DeviceName) == "" {
		host, err := os.Hostname()
		if err != nil || host == "" {
			host = "Linux-Mac-Bridge"
		}
		c.DeviceName = host
	}
	// macOS Gemini.app 프로토콜 호환을 위해 기본값은 "macOS"로 유지한다.
	if strings.TrimSpace(c.OSType) == "" {
		c.OSType = "macOS"
	}
	if strings.TrimSpace(c.OSVersion) == "" {
		c.OSVersion = DefaultOSVersion
	}
	if strings.TrimSpace(c.OSBuild) == "" {
		c.OSBuild = DefaultOSBuild
	}
	if strings.TrimSpace(c.AppVersion) == "" {
		c.AppVersion = DefaultAppVersion
	}
	if strings.TrimSpace(c.Locale) == "" {
		c.Locale = "ko-KR"
	}
	if strings.TrimSpace(c.TimeZone) == "" {
		c.TimeZone = detectTimeZone()
	}
	if len(c.ConnectedFolders) == 0 {
		home, _ := os.UserHomeDir()
		if home != "" {
			gitRepo := filepath.Join(home, "dev", "git-repo")
			if st, err := os.Stat(gitRepo); err == nil && st.IsDir() {
				c.ConnectedFolders = []string{gitRepo}
			} else {
				c.ConnectedFolders = []string{home}
			}
		}
	}
	if len(c.MCPServers) == 0 {
		c.MCPServers = []MCPServerConfig{
			{
				ServerID:    "git",
				DisplayName: "Git",
				Instruction: "Local Git repository inspection and management tools",
				Enabled:     true,
			},
			{
				ServerID:    "obsidian",
				DisplayName: "Obsidian",
				Instruction: "Local Markdown and Obsidian note search and editing tools",
				Enabled:     true,
			},
		}
	}
}

// Save는 설정을 디스크에 원자적으로 기록한다.
func (c *Config) Save() error {
	p, err := ConfigPath()
	if err != nil {
		return err
	}
	c.mu.RLock()
	data, err := json.MarshalIndent(c, "", "  ")
	c.mu.RUnlock()
	if err != nil {
		return err
	}
	return writeFileAtomic(p, data, 0o600)
}

func writeFileAtomic(path string, data []byte, perm os.FileMode) error {
	dir := filepath.Dir(path)
	f, err := os.CreateTemp(dir, "."+filepath.Base(path)+".*.tmp")
	if err != nil {
		return err
	}
	tmp := f.Name()
	cleanup := func() {
		_ = f.Close()
		_ = os.Remove(tmp)
	}
	if err := f.Chmod(perm); err != nil {
		cleanup()
		return err
	}
	if _, err := f.Write(data); err != nil {
		cleanup()
		return err
	}
	if err := f.Close(); err != nil {
		_ = os.Remove(tmp)
		return err
	}
	if err := os.Rename(tmp, path); err != nil {
		_ = os.Remove(tmp)
		return err
	}
	return nil
}

// CollectBootstrapFiles는 연결된 폴더 및 홈 디렉토리의 AGENTS.md / CLAUDE.md / GEMINI.md 파일을 수집한다.
// macOS Gemini.app과 동일하게 심볼릭 링크는 건너뛰고 32,768바이트까지만 읽는다.
func (c *Config) CollectBootstrapFiles() map[string][]byte {
	out := make(map[string][]byte)
	home, _ := os.UserHomeDir()

	candidates := []struct {
		key  string
		path string
	}{}

	if home != "" {
		candidates = append(candidates,
			struct{ key, path string }{"~/.gemini/AGENTS.md", filepath.Join(home, ".gemini", "AGENTS.md")},
			struct{ key, path string }{"~/.gemini/GEMINI.md", filepath.Join(home, ".gemini", "GEMINI.md")},
			struct{ key, path string }{"~/AGENTS.md", filepath.Join(home, "AGENTS.md")},
		)
	}
	for _, folder := range c.CloneConnectedFolders() {
		for _, name := range []string{"AGENTS.md", "CLAUDE.md", "GEMINI.md"} {
			full := filepath.Join(folder, name)
			candidates = append(candidates, struct{ key, path string }{full, full})
		}
	}

	for _, cand := range candidates {
		fi, err := os.Lstat(cand.path)
		if err != nil || fi.Mode()&os.ModeSymlink != 0 || !fi.Mode().IsRegular() {
			continue
		}
		data, err := os.ReadFile(cand.path)
		if err != nil || len(data) == 0 {
			continue
		}
		if len(data) > MaxBootstrapFileSize {
			data = data[:MaxBootstrapFileSize]
		}
		out[cand.key] = data
	}
	const remoteFileNote = "\n\n## Remote File & Image Sharing Rule\n- To read local source code, configuration files (e.g., `pom.xml`, `.json`, `.yaml`, `.md`), or logs, use `read_file` (or `view_file`) and present relevant snippets or summaries directly in Markdown in the chat. NEVER upload source code, config files, or sensitive files to Google Drive unless the user explicitly asks to upload them.\n- ONLY call `upload_to_drive` when the user explicitly asks you to show, view, or share an image, screenshot, PDF, or document in the Gemini app, and include the returned `web_view_link` as a clickable Markdown link. NEVER use `run_command` with `base64` to encode images.\n"
	for _, targetKey := range []string{"~/.gemini/GEMINI.md", "~/AGENTS.md"} {
		if existing, ok := out[targetKey]; ok && len(existing)+len(remoteFileNote) <= MaxBootstrapFileSize {
			out[targetKey] = append(existing, []byte(remoteFileNote)...)
		}
	}
	return out
}

func detectTimeZone() string {
	if tz := os.Getenv("TZ"); tz != "" {
		return tz
	}
	if target, err := os.Readlink("/etc/localtime"); err == nil {
		if idx := strings.Index(target, "zoneinfo/"); idx != -1 {
			return target[idx+len("zoneinfo/"):]
		}
	}
	return "Asia/Seoul"
}

func newUUIDv4() string {
	var b [16]byte
	_, _ = rand.Read(b[:])
	b[6] = (b[6] & 0x0f) | 0x40
	b[8] = (b[8] & 0x3f) | 0x80
	return fmt.Sprintf("%08x-%04x-%04x-%04x-%012x",
		b[0:4], b[4:6], b[6:8], b[8:10], b[10:16])
}
