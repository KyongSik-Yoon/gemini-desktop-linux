package mcp

import (
	"bufio"
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"io"
	"os"
	"os/exec"
	"path/filepath"
	"sort"
	"strings"
	"sync"
	"sync/atomic"
	"time"

	"github.com/sam/gemini-remote-linux/internal/config"
	"github.com/sam/gemini-remote-linux/internal/proto"
)

// BundledServer는 DesktopDeviceInfo.bundled_mcp_servers에 포함될 서버 선언이다.
type BundledServer struct {
	ServerID         string
	DisplayName      string
	Instruction      string
	Enabled          bool
	ToolDeclarations []proto.FunctionDecl
}

// Registry는 내장(git, obsidian) 및 외부 stdio MCP 서버를 관리하고 호출을 중계한다.
type Registry struct {
	mu            sync.RWMutex
	cfg           *config.Config
	servers       map[string]*BundledServer
	reqSeq        atomic.Int64
	stdioMu       sync.Mutex
	stdioSessions map[string]*mcpStdioSession
}

func NewRegistry(cfg *config.Config) *Registry {
	r := &Registry{
		cfg:           cfg,
		servers:       make(map[string]*BundledServer),
		stdioSessions: make(map[string]*mcpStdioSession),
	}
	r.reload()
	return r
}

// Close는 열린 외부 stdio MCP 세션을 모두 종료한다.
func (r *Registry) Close() {
	r.stdioMu.Lock()
	sessions := r.stdioSessions
	r.stdioSessions = make(map[string]*mcpStdioSession)
	r.stdioMu.Unlock()
	for _, sess := range sessions {
		sess.close()
	}
}

func (r *Registry) reload() {
	r.mu.Lock()
	defer r.mu.Unlock()

	for _, sc := range r.cfg.CloneMCPServers() {
		if !sc.Enabled {
			continue
		}
		switch sc.ServerID {
		case "git":
			r.servers["git"] = &BundledServer{
				ServerID:         "git",
				DisplayName:       nonEmpty(sc.DisplayName, "Git"),
				Instruction:      nonEmpty(sc.Instruction, "Inspect and manage local Git repositories."),
				Enabled:          true,
				ToolDeclarations: gitToolDeclarations(),
			}
		case "obsidian":
			r.servers["obsidian"] = &BundledServer{
				ServerID:         "obsidian",
				DisplayName:      nonEmpty(sc.DisplayName, "Obsidian"),
				Instruction:      nonEmpty(sc.Instruction, "Read, search, and update local Markdown notes."),
				Enabled:          true,
				ToolDeclarations: obsidianToolDeclarations(),
			}
		default:
			decls := r.probeExternalMCPTools(sc)
			r.servers[sc.ServerID] = &BundledServer{
				ServerID:         sc.ServerID,
				DisplayName:      nonEmpty(sc.DisplayName, sc.ServerID),
				Instruction:      sc.Instruction,
				Enabled:          true,
				ToolDeclarations: decls,
			}
		}
	}
}

// BundledServers는 등록할 BundledMcpServer 목록을 반환한다.
func (r *Registry) BundledServers() []BundledServer {
	r.mu.RLock()
	defer r.mu.RUnlock()

	keys := make([]string, 0, len(r.servers))
	for k := range r.servers {
		keys = append(keys, k)
	}
	sort.Strings(keys)

	out := make([]BundledServer, 0, len(keys))
	for _, k := range keys {
		out = append(out, *r.servers[k])
	}
	return out
}

// ListServersJSON은 list_mcp 도구 응답용 맵을 반환한다.
func (r *Registry) ListServersJSON() map[string]any {
	servers := r.BundledServers()
	items := make([]map[string]any, 0, len(servers))
	for _, s := range servers {
		tools := make([]string, 0, len(s.ToolDeclarations))
		for _, t := range s.ToolDeclarations {
			tools = append(tools, t.Name)
		}
		items = append(items, map[string]any{
			"server_id":    s.ServerID,
			"display_name": s.DisplayName,
			"instruction":  s.Instruction,
			"enabled":      s.Enabled,
			"tools":        tools,
		})
	}
	return map[string]any{"mcp_servers": items}
}

// Execute는 내장 또는 외부 MCP 도구를 실행한다.
func (r *Registry) Execute(ctx context.Context, serverID, toolName string, args map[string]any) (map[string]any, error) {
	if serverID == "" {
		// 도구 이름으로 서버 자동 추론
		if strings.HasPrefix(toolName, "git_") {
			serverID = "git"
		} else if strings.HasPrefix(toolName, "obsidian_") {
			serverID = "obsidian"
		} else if parts := strings.SplitN(toolName, "__", 2); len(parts) == 2 {
			serverID = parts[0]
			toolName = parts[1]
		}
	}

	switch serverID {
	case "git":
		return r.execGitTool(ctx, toolName, args)
	case "obsidian":
		return r.execObsidianTool(ctx, toolName, args)
	default:
		for _, sc := range r.cfg.CloneMCPServers() {
			if sc.ServerID == serverID && sc.Command != "" {
				return r.callExternalMCP(ctx, sc, toolName, args)
			}
		}
		return nil, fmt.Errorf("알 수 없는 MCP 서버 또는 도구: server=%q tool=%q", serverID, toolName)
	}
}

func (r *Registry) defaultRepoPath(args map[string]any) string {
	for _, k := range []string{"repo_path", "repository", "path", "working_directory"} {
		if v, ok := args[k].(string); ok && strings.TrimSpace(v) != "" {
			return v
		}
	}
	if first := r.cfg.FirstConnectedFolder(); first != "" {
		return first
	}
	wd, _ := os.Getwd()
	return wd
}

func (r *Registry) execGitTool(ctx context.Context, toolName string, args map[string]any) (map[string]any, error) {
	repo := r.defaultRepoPath(args)
	cctx, cancel := context.WithTimeout(ctx, 20*time.Second)
	defer cancel()

	runGit := func(gitArgs ...string) (map[string]any, error) {
		cmd := exec.CommandContext(cctx, "git", append([]string{"-C", repo}, gitArgs...)...)
		out, err := cmd.CombinedOutput()
		res := map[string]any{
			"repository": repo,
			"output":     string(out),
		}
		if err != nil {
			res["error"] = err.Error()
		}
		return res, nil
	}

	switch toolName {
	case "git_status":
		return runGit("status", "-sb")
	case "git_diff":
		staged, _ := args["staged"].(bool)
		if staged {
			return runGit("diff", "--staged")
		}
		if target, ok := args["target"].(string); ok && target != "" {
			return runGit("diff", target)
		}
		return runGit("diff")
	case "git_log":
		limit := "15"
		if n, ok := args["max_count"].(float64); ok && n > 0 {
			limit = fmt.Sprintf("%d", int(n))
		}
		return runGit("log", "-n", limit, "--oneline", "--decorate")
	case "git_show":
		rev := "HEAD"
		if v, ok := args["revision"].(string); ok && v != "" {
			rev = v
		}
		return runGit("show", rev)
	case "git_branch":
		return runGit("branch", "-a")
	case "git_checkout":
		branch, _ := args["branch"].(string)
		if branch == "" {
			return nil, fmt.Errorf("branch 파라미터가 필요합니다")
		}
		return runGit("checkout", branch)
	case "git_commit":
		msg, _ := args["message"].(string)
		if msg == "" {
			return nil, fmt.Errorf("message 파라미터가 필요합니다")
		}
		return runGit("commit", "-m", msg)
	default:
		return nil, fmt.Errorf("지원하지 않는 git MCP 도구: %s", toolName)
	}
}

func (r *Registry) allowedVaultRoots() []string {
	var roots []string
	if v := strings.TrimSpace(r.cfg.GetObsidianVaultDir()); v != "" {
		roots = append(roots, v)
	}
	roots = append(roots, r.cfg.CloneConnectedFolders()...)
	if home, err := os.UserHomeDir(); err == nil && home != "" {
		roots = append(roots, filepath.Join(home, "Documents"))
	}
	return roots
}

func isWithinRoot(root, candidate string) bool {
	rootAbs, err := filepath.Abs(root)
	if err != nil {
		return false
	}
	if resolved, err := filepath.EvalSymlinks(rootAbs); err == nil {
		rootAbs = resolved
	}
	candAbs, err := filepath.Abs(candidate)
	if err != nil {
		return false
	}
	if resolved, err := filepath.EvalSymlinks(candAbs); err == nil {
		candAbs = resolved
	} else {
		// 아직 존재하지 않는 파일(쓰기 대상)은 가장 가까운 기존 상위 디렉터리를 심볼릭 해석한다.
		cur := candAbs
		var tail []string
		for {
			parent := filepath.Dir(cur)
			if parent == cur {
				break
			}
			tail = append([]string{filepath.Base(cur)}, tail...)
			cur = parent
			if resolvedParent, rerr := filepath.EvalSymlinks(cur); rerr == nil {
				candAbs = filepath.Join(append([]string{resolvedParent}, tail...)...)
				break
			}
		}
	}
	rel, err := filepath.Rel(rootAbs, candAbs)
	if err != nil {
		return false
	}
	return rel == "." || (rel != ".." && !strings.HasPrefix(rel, ".."+string(os.PathSeparator)))
}

func (r *Registry) vaultRoot(args map[string]any) (string, error) {
	if v, ok := args["vault_path"].(string); ok && strings.TrimSpace(v) != "" {
		for _, allowed := range r.allowedVaultRoots() {
			if isWithinRoot(allowed, v) {
				abs, err := filepath.Abs(v)
				if err != nil {
					return "", err
				}
				return abs, nil
			}
		}
		return "", fmt.Errorf("허용되지 않은 vault_path입니다: %s", v)
	}
	if v := strings.TrimSpace(r.cfg.GetObsidianVaultDir()); v != "" {
		return v, nil
	}
	if first := r.cfg.FirstConnectedFolder(); first != "" {
		return first, nil
	}
	home, _ := os.UserHomeDir()
	return filepath.Join(home, "Documents"), nil
}

func safeVaultPath(vault, note string) (string, error) {
	note = strings.TrimSpace(note)
	if note == "" || strings.IndexByte(note, 0) != -1 {
		return "", fmt.Errorf("note_path가 유효하지 않습니다")
	}
	if filepath.IsAbs(note) {
		return "", fmt.Errorf("볼트 상대 경로만 허용됩니다: %s", note)
	}
	full := filepath.Join(vault, note)
	if !isWithinRoot(vault, full) {
		return "", fmt.Errorf("볼트 범위를 벗어난 경로는 접근할 수 없습니다: %s", note)
	}
	return full, nil
}

func (r *Registry) execObsidianTool(_ context.Context, toolName string, args map[string]any) (map[string]any, error) {
	vault, err := r.vaultRoot(args)
	if err != nil {
		return nil, err
	}
	switch toolName {
	case "obsidian_list_notes":
		var notes []string
		_ = filepath.WalkDir(vault, func(path string, d os.DirEntry, err error) error {
			if err != nil {
				return nil
			}
			if d.IsDir() && (strings.HasPrefix(d.Name(), ".") || d.Name() == "node_modules") {
				return filepath.SkipDir
			}
			if !d.IsDir() && strings.HasSuffix(strings.ToLower(d.Name()), ".md") {
				rel, err := filepath.Rel(vault, path)
				if err == nil {
					notes = append(notes, rel)
				}
			}
			if len(notes) >= 200 {
				return io.EOF
			}
			return nil
		})
		return map[string]any{"vault": vault, "notes": notes}, nil

	case "obsidian_read_note":
		note, _ := args["note_path"].(string)
		full, err := safeVaultPath(vault, note)
		if err != nil {
			return map[string]any{"error": err.Error()}, nil
		}
		data, err := os.ReadFile(full)
		if err != nil {
			return map[string]any{"error": err.Error()}, nil
		}
		return map[string]any{"note_path": note, "content": string(data)}, nil

	case "obsidian_write_note":
		note, _ := args["note_path"].(string)
		content, _ := args["content"].(string)
		full, err := safeVaultPath(vault, note)
		if err != nil {
			return map[string]any{"error": err.Error()}, nil
		}
		if err := os.MkdirAll(filepath.Dir(full), 0o755); err != nil {
			return map[string]any{"error": err.Error()}, nil
		}
		if err := os.WriteFile(full, []byte(content), 0o644); err != nil {
			return map[string]any{"error": err.Error()}, nil
		}
		return map[string]any{"success": true, "note_path": note}, nil

	case "obsidian_search_notes":
		query, _ := args["query"].(string)
		qLower := strings.ToLower(query)
		var matches []map[string]any
		_ = filepath.WalkDir(vault, func(path string, d os.DirEntry, err error) error {
			if err != nil {
				return nil
			}
			if d.IsDir() && (strings.HasPrefix(d.Name(), ".") || d.Name() == "node_modules") {
				return filepath.SkipDir
			}
			if !d.IsDir() && strings.HasSuffix(strings.ToLower(d.Name()), ".md") {
				data, err := os.ReadFile(path)
				if err == nil && strings.Contains(strings.ToLower(string(data)), qLower) {
					rel, _ := filepath.Rel(vault, path)
					matches = append(matches, map[string]any{"note_path": rel})
				}
			}
			if len(matches) >= 50 {
				return io.EOF
			}
			return nil
		})
		return map[string]any{"query": query, "matches": matches}, nil

	default:
		return nil, fmt.Errorf("지원하지 않는 obsidian MCP 도구: %s", toolName)
	}
}

const mcpStdioIdleTimeout = 2 * time.Minute

// mcpStdioSession은 외부 stdio MCP 서버 프로세스를 유지·재사용해 매 호출마다 프로세스를 재기동하는 비용을 없앤다.
type mcpStdioSession struct {
	mu        sync.Mutex
	serverID  string
	cmd       *exec.Cmd
	stdin     io.WriteCloser
	reader    *bufio.Reader
	closed    bool
	idleTimer *time.Timer
	onIdle    func(*mcpStdioSession)
}

func (s *mcpStdioSession) close() {
	s.mu.Lock()
	defer s.mu.Unlock()
	s.closeLocked()
}

func (s *mcpStdioSession) closeLocked() {
	if s.closed {
		return
	}
	s.closed = true
	if s.idleTimer != nil {
		s.idleTimer.Stop()
		s.idleTimer = nil
	}
	if s.stdin != nil {
		_ = s.stdin.Close()
	}
	if s.cmd != nil && s.cmd.Process != nil {
		_ = s.cmd.Process.Kill()
		_ = s.cmd.Wait()
	}
}

func (s *mcpStdioSession) resetIdleTimerLocked() {
	if s.idleTimer != nil {
		s.idleTimer.Stop()
	}
	s.idleTimer = time.AfterFunc(mcpStdioIdleTimeout, func() {
		if s.onIdle != nil {
			s.onIdle(s)
		}
		s.close()
	})
}

func (s *mcpStdioSession) sendLocked(id int64, method string, params map[string]any) error {
	payload := map[string]any{
		"jsonrpc": "2.0",
		"method":  method,
		"params":  params,
	}
	if id > 0 {
		payload["id"] = id
	}
	b, err := json.Marshal(payload)
	if err != nil {
		return err
	}
	b = append(b, '\n')
	_, err = s.stdin.Write(b)
	return err
}

func (s *mcpStdioSession) readResponseLocked(ctx context.Context, targetID int64) (map[string]any, map[string]any, error) {
	type readRes struct {
		result map[string]any
		rpcErr map[string]any
		err    error
	}
	ch := make(chan readRes, 1)
	go func() {
		for {
			line, err := s.reader.ReadBytes('\n')
			if err != nil {
				ch <- readRes{err: err}
				return
			}
			line = bytes.TrimSpace(line)
			if len(line) == 0 {
				continue
			}
			var msg struct {
				ID     int64          `json:"id"`
				Result map[string]any `json:"result"`
				Error  map[string]any `json:"error"`
			}
			if json.Unmarshal(line, &msg) == nil && msg.ID == targetID {
				ch <- readRes{result: msg.Result, rpcErr: msg.Error}
				return
			}
		}
	}()

	select {
	case res := <-ch:
		return res.result, res.rpcErr, res.err
	case <-ctx.Done():
		// 블로킹된 ReadBytes 고루틴을 해제하기 위해 프로세스를 즉시 종료한다.
		s.closeLocked()
		<-ch
		return nil, nil, ctx.Err()
	}
}

func jsonSchemaToProto(m map[string]any) *proto.SchemaDef {
	if len(m) == 0 {
		return &proto.SchemaDef{Type: proto.TypeObject}
	}
	sd := &proto.SchemaDef{}
	typeStr, _ := m["type"].(string)
	if typeStr == "" {
		// JSON Schema 2020-12 스타일의 type: ["string", "null"] 배열도 지원한다.
		if arr, ok := m["type"].([]any); ok {
			for _, v := range arr {
				s, _ := v.(string)
				if s == "null" {
					sd.Nullable = true
				} else if typeStr == "" && s != "" {
					typeStr = s
				}
			}
		}
	}
	switch strings.ToLower(strings.TrimSpace(typeStr)) {
	case "string":
		sd.Type = proto.TypeString
	case "integer":
		sd.Type = proto.TypeInteger
	case "number":
		sd.Type = proto.TypeNumber
	case "boolean":
		sd.Type = proto.TypeBoolean
	case "array":
		sd.Type = proto.TypeArray
	case "object":
		sd.Type = proto.TypeObject
	default:
		if _, hasProps := m["properties"]; hasProps {
			sd.Type = proto.TypeObject
		} else {
			sd.Type = proto.TypeUnspecified
		}
	}
	if desc, ok := m["description"].(string); ok {
		sd.Description = desc
	}
	if nullable, ok := m["nullable"].(bool); ok && nullable {
		sd.Nullable = true
	}
	if enumAny, ok := m["enum"].([]any); ok {
		for _, v := range enumAny {
			if s, ok := v.(string); ok {
				sd.Enum = append(sd.Enum, s)
			} else if v != nil {
				sd.Enum = append(sd.Enum, fmt.Sprint(v))
			}
		}
	}
	if itemsMap, ok := m["items"].(map[string]any); ok {
		sd.Items = jsonSchemaToProto(itemsMap)
	}
	if propsMap, ok := m["properties"].(map[string]any); ok && len(propsMap) > 0 {
		sd.Properties = make(map[string]*proto.SchemaDef, len(propsMap))
		for k, v := range propsMap {
			if vm, ok := v.(map[string]any); ok {
				sd.Properties[k] = jsonSchemaToProto(vm)
			}
		}
	}
	if reqAny, ok := m["required"].([]any); ok {
		for _, v := range reqAny {
			if s, ok := v.(string); ok && s != "" {
				sd.Required = append(sd.Required, s)
			}
		}
	}
	return sd
}

func (r *Registry) probeExternalMCPTools(sc config.MCPServerConfig) []proto.FunctionDecl {
	if sc.Command == "" {
		return nil
	}
	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()
	resp, err := r.runStdioRPC(ctx, sc, "tools/list", map[string]any{})
	if err != nil {
		return nil
	}
	toolsAny, _ := resp["tools"].([]any)
	var out []proto.FunctionDecl
	for _, t := range toolsAny {
		tm, ok := t.(map[string]any)
		if !ok {
			continue
		}
		name, _ := tm["name"].(string)
		desc, _ := tm["description"].(string)
		if name != "" {
			var params *proto.SchemaDef
			if schemaMap, ok := tm["inputSchema"].(map[string]any); ok {
				params = jsonSchemaToProto(schemaMap)
			} else {
				params = &proto.SchemaDef{Type: proto.TypeObject}
			}
			out = append(out, proto.FunctionDecl{
				Name:        name,
				Description: desc,
				Parameters:  params,
			})
		}
	}
	return out
}

func (r *Registry) callExternalMCP(ctx context.Context, sc config.MCPServerConfig, toolName string, args map[string]any) (map[string]any, error) {
	return r.runStdioRPC(ctx, sc, "tools/call", map[string]any{
		"name":      toolName,
		"arguments": args,
	})
}

func (r *Registry) evictStdioSession(serverID string, target *mcpStdioSession) {
	r.stdioMu.Lock()
	if cur, ok := r.stdioSessions[serverID]; ok && (target == nil || cur == target) {
		delete(r.stdioSessions, serverID)
	}
	r.stdioMu.Unlock()
}

func (r *Registry) getOrCreateStdioSession(ctx context.Context, sc config.MCPServerConfig) (*mcpStdioSession, error) {
	serverKey := sc.ServerID
	if serverKey == "" {
		serverKey = sc.Command
	}

	r.stdioMu.Lock()
	if r.stdioSessions == nil {
		r.stdioSessions = make(map[string]*mcpStdioSession)
	}
	if existing, ok := r.stdioSessions[serverKey]; ok {
		r.stdioMu.Unlock()
		return existing, nil
	}
	r.stdioMu.Unlock()

	cmd := exec.Command(sc.Command, sc.Args...)
	cmd.Env = os.Environ()
	for k, v := range sc.Env {
		cmd.Env = append(cmd.Env, fmt.Sprintf("%s=%s", k, v))
	}
	stdin, err := cmd.StdinPipe()
	if err != nil {
		return nil, err
	}
	stdout, err := cmd.StdoutPipe()
	if err != nil {
		_ = stdin.Close()
		return nil, err
	}
	if err := cmd.Start(); err != nil {
		_ = stdin.Close()
		return nil, err
	}

	sess := &mcpStdioSession{
		serverID: serverKey,
		cmd:      cmd,
		stdin:    stdin,
		reader:   bufio.NewReader(stdout),
		onIdle: func(s *mcpStdioSession) {
			r.evictStdioSession(serverKey, s)
		},
	}

	// MCP initialize 핸드셰이크 및 JSON-RPC 에러 검증
	sess.mu.Lock()
	if err := sess.sendLocked(1, "initialize", map[string]any{
		"protocolVersion": "2024-11-05",
		"capabilities":    map[string]any{},
		"clientInfo":      map[string]any{"name": "gemini-remote-linux", "version": "1.0.0"},
	}); err != nil {
		sess.closeLocked()
		sess.mu.Unlock()
		return nil, err
	}
	_, initErr, err := sess.readResponseLocked(ctx, 1)
	if err != nil {
		sess.closeLocked()
		sess.mu.Unlock()
		return nil, fmt.Errorf("MCP initialize 실패: %w", err)
	}
	if initErr != nil {
		sess.closeLocked()
		sess.mu.Unlock()
		return nil, fmt.Errorf("MCP initialize 오류: %v", initErr)
	}
	if err := sess.sendLocked(0, "notifications/initialized", map[string]any{}); err != nil {
		sess.closeLocked()
		sess.mu.Unlock()
		return nil, err
	}
	sess.resetIdleTimerLocked()
	sess.mu.Unlock()

	r.stdioMu.Lock()
	if existing, ok := r.stdioSessions[serverKey]; ok {
		r.stdioMu.Unlock()
		sess.close()
		return existing, nil
	}
	r.stdioSessions[serverKey] = sess
	r.stdioMu.Unlock()
	return sess, nil
}

func (r *Registry) runStdioRPC(ctx context.Context, sc config.MCPServerConfig, method string, params map[string]any) (map[string]any, error) {
	serverKey := sc.ServerID
	if serverKey == "" {
		serverKey = sc.Command
	}

	var lastErr error
	for attempt := 0; attempt < 2; attempt++ {
		sess, err := r.getOrCreateStdioSession(ctx, sc)
		if err != nil {
			return nil, err
		}

		sess.mu.Lock()
		if sess.closed {
			sess.mu.Unlock()
			r.evictStdioSession(serverKey, sess)
			lastErr = io.EOF
			continue
		}
		if sess.idleTimer != nil {
			sess.idleTimer.Stop()
		}
		reqID := r.reqSeq.Add(1) + 10
		if err := sess.sendLocked(reqID, method, params); err != nil {
			sess.closeLocked()
			sess.mu.Unlock()
			r.evictStdioSession(serverKey, sess)
			lastErr = err
			continue
		}
		res, rpcErr, err := sess.readResponseLocked(ctx, reqID)
		if err != nil {
			sess.closeLocked()
			sess.mu.Unlock()
			r.evictStdioSession(serverKey, sess)
			if ctx.Err() != nil {
				return nil, err
			}
			lastErr = err
			continue
		}
		sess.resetIdleTimerLocked()
		sess.mu.Unlock()

		if rpcErr != nil {
			return map[string]any{"error": rpcErr}, nil
		}
		return res, nil
	}
	return nil, lastErr
}

func gitToolDeclarations() []proto.FunctionDecl {
	repoProp := map[string]*proto.SchemaDef{
		"repo_path": {Type: proto.TypeString, Description: "Absolute path to the Git repository"},
	}
	return []proto.FunctionDecl{
		{
			Name:        "git_status",
			Description: "Show the working tree status of a Git repository.",
			Parameters:  &proto.SchemaDef{Type: proto.TypeObject, Properties: repoProp},
		},
		{
			Name:        "git_diff",
			Description: "Show changes between commits or working tree.",
			Parameters: &proto.SchemaDef{
				Type: proto.TypeObject,
				Properties: map[string]*proto.SchemaDef{
					"repo_path": {Type: proto.TypeString, Description: "Repository path"},
					"staged":    {Type: proto.TypeBoolean, Description: "Show staged changes"},
					"target":    {Type: proto.TypeString, Description: "Optional revision or path"},
				},
			},
		},
		{
			Name:        "git_log",
			Description: "Show commit logs.",
			Parameters: &proto.SchemaDef{
				Type: proto.TypeObject,
				Properties: map[string]*proto.SchemaDef{
					"repo_path": {Type: proto.TypeString, Description: "Repository path"},
					"max_count": {Type: proto.TypeInteger, Description: "Maximum number of commits"},
				},
			},
		},
		{
			Name:        "git_show",
			Description: "Show commit details.",
			Parameters: &proto.SchemaDef{
				Type: proto.TypeObject,
				Properties: map[string]*proto.SchemaDef{
					"repo_path": {Type: proto.TypeString, Description: "Repository path"},
					"revision":  {Type: proto.TypeString, Description: "Commit revision"},
				},
			},
		},
		{
			Name:        "git_branch",
			Description: "List branches in the repository.",
			Parameters:  &proto.SchemaDef{Type: proto.TypeObject, Properties: repoProp},
		},
	}
}

func obsidianToolDeclarations() []proto.FunctionDecl {
	return []proto.FunctionDecl{
		{
			Name:        "obsidian_list_notes",
			Description: "List Markdown notes in the Obsidian vault.",
			Parameters: &proto.SchemaDef{
				Type: proto.TypeObject,
				Properties: map[string]*proto.SchemaDef{
					"vault_path": {Type: proto.TypeString, Description: "Optional vault root path"},
				},
			},
		},
		{
			Name:        "obsidian_read_note",
			Description: "Read a Markdown note from the Obsidian vault.",
			Parameters: &proto.SchemaDef{
				Type:     proto.TypeObject,
				Required: []string{"note_path"},
				Properties: map[string]*proto.SchemaDef{
					"note_path":  {Type: proto.TypeString, Description: "Relative path to the note"},
					"vault_path": {Type: proto.TypeString, Description: "Optional vault root path"},
				},
			},
		},
		{
			Name:        "obsidian_write_note",
			Description: "Create or overwrite a Markdown note in the Obsidian vault.",
			Parameters: &proto.SchemaDef{
				Type:     proto.TypeObject,
				Required: []string{"note_path", "content"},
				Properties: map[string]*proto.SchemaDef{
					"note_path":  {Type: proto.TypeString, Description: "Relative path to the note"},
					"content":    {Type: proto.TypeString, Description: "Markdown content"},
					"vault_path": {Type: proto.TypeString, Description: "Optional vault root path"},
				},
			},
		},
		{
			Name:        "obsidian_search_notes",
			Description: "Search Markdown notes in the Obsidian vault.",
			Parameters: &proto.SchemaDef{
				Type:     proto.TypeObject,
				Required: []string{"query"},
				Properties: map[string]*proto.SchemaDef{
					"query":      {Type: proto.TypeString, Description: "Search query"},
					"vault_path": {Type: proto.TypeString, Description: "Optional vault root path"},
				},
			},
		},
	}
}

func nonEmpty(v, fallback string) string {
	if strings.TrimSpace(v) != "" {
		return v
	}
	return fallback
}
