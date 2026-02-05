import React, { useState, useEffect, useCallback, useRef } from 'react';
import { useParams, useNavigate, useLocation } from 'react-router-dom';
import CodeMirror from '@uiw/react-codemirror';
import { markdown } from '@codemirror/lang-markdown';
import { EditorView } from '@codemirror/view';
import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';
import { updateFileMeta, onFilesChanged, simpleHash, logout, auth } from '../firebase';

const API = '/api';
const AUTO_SAVE_DELAY = 1000;

// API 경로 인코딩 헬퍼 (한글 등 유니코드 지원, / 유지)
const encodePath = (p) => p.split('/').map(s => encodeURIComponent(s)).join('/');

// 인증 헤더 생성 헬퍼
function authHeaders() {
  const headers = { 'Content-Type': 'application/json' };
  if (auth.currentUser) {
    headers['X-Firebase-UID'] = auth.currentUser.uid;
  }
  return headers;
}

const darkTheme = EditorView.theme({
  '&': { backgroundColor: '#0d1117', color: '#e6edf3' },
  '.cm-content': { caretColor: '#58a6ff' },
  '.cm-cursor': { borderLeftColor: '#58a6ff' },
  '&.cm-focused .cm-selectionBackground, .cm-selectionBackground': { backgroundColor: '#1f6feb44' },
  '.cm-gutters': { backgroundColor: '#161b22', color: '#484f58', border: 'none' },
  '.cm-activeLineGutter': { backgroundColor: '#1f6feb22' },
  '.cm-activeLine': { backgroundColor: '#1f6feb11' },
}, { dark: true });

// 토스트 알림 컴포넌트
function Toast({ toasts, onRemove }) {
  return (
    <div className="toast-container">
      {toasts.map(t => (
        <div key={t.id} className={`toast toast-${t.type}`} onClick={() => onRemove(t.id)}>
          <span className="toast-icon">
            {t.type === 'loading' ? '⏳' : t.type === 'success' ? '✅' : '❌'}
          </span>
          <span className="toast-msg">{t.message}</span>
        </div>
      ))}
    </div>
  );
}

export default function Workspace({ user }) {
  const { userId, '*': filePath } = useParams();
  const navigate = useNavigate();
  const location = useLocation();

  const [files, setFiles] = useState([]);
  const [currentFile, setCurrentFile] = useState(null);
  const [content, setContent] = useState('');
  const [savedContent, setSavedContent] = useState('');
  const [view, setView] = useState('edit');
  const [saveStatus, setSaveStatus] = useState('idle');
  const [contextMenu, setContextMenu] = useState(null);
  const [sidebarOpen, setSidebarOpen] = useState(false);
  const [toasts, setToasts] = useState([]);
  const [sidebarLoading, setSidebarLoading] = useState(false);
  const [focusedFolder, setFocusedFolder] = useState('');
  const saveTimer = useRef(null);
  const toastId = useRef(0);

  // 토스트 헬퍼
  const addToast = useCallback((message, type = 'loading', duration = null) => {
    const id = ++toastId.current;
    setToasts(prev => [...prev, { id, message, type }]);
    if (duration) setTimeout(() => setToasts(prev => prev.filter(t => t.id !== id)), duration);
    return id;
  }, []);

  const updateToast = useCallback((id, message, type, duration = 2000) => {
    setToasts(prev => prev.map(t => t.id === id ? { ...t, message, type } : t));
    if (duration) setTimeout(() => setToasts(prev => prev.filter(t => t.id !== id)), duration);
  }, []);

  const removeToast = useCallback((id) => {
    setToasts(prev => prev.filter(t => t.id !== id));
  }, []);

  // 파일 트리 로드
  const loadFiles = useCallback(async () => {
    try {
      const r = await fetch(`${API}/${userId}/files`);
      const data = await r.json();
      setFiles(data.files || []);
    } catch (err) {
      console.error('Failed to load files:', err);
    }
  }, [userId]);

  useEffect(() => { loadFiles(); }, [loadFiles]);

  // URL 경로에서 파일 열기 (전환 시 즉시 클리어 후 로딩)
  useEffect(() => {
    if (filePath) {
      const fp = decodeURIComponent(filePath);
      // 즉시 기존 내용 클리어
      setContent('');
      setSavedContent('');
      setCurrentFile({ path: fp, loading: true });
      setSaveStatus('idle');
      fetch(`${API}/${userId}/file/${encodePath(fp)}`)
        .then(r => r.json())
        .then(data => {
          if (!data.error) {
            setCurrentFile(data);
            setContent(data.content);
            setSavedContent(data.content);
          }
        })
        .catch(() => {});
    } else {
      setCurrentFile(null);
      setContent('');
      setSavedContent('');
    }
  }, [filePath, userId]);

  // 컨텍스트 메뉴 닫기
  useEffect(() => {
    const handler = () => setContextMenu(null);
    window.addEventListener('click', handler);
    return () => window.removeEventListener('click', handler);
  }, []);

  // Firebase 변경 감지
  useEffect(() => {
    const unsubscribe = onFilesChanged(userId, (changedFiles) => {
      if (currentFile) {
        const changed = changedFiles.find(f => f.path === currentFile.path);
        if (changed && changed.hash !== simpleHash(content)) {
          fetch(`${API}/${userId}/file/${encodePath(currentFile.path)}`)
            .then(r => r.json())
            .then(data => {
              setContent(data.content);
              setSavedContent(data.content);
              setSaveStatus('idle');
            })
            .catch(err => console.error('Failed to reload:', err));
        }
      }
      loadFiles();
    });
    return () => unsubscribe && unsubscribe();
  }, [currentFile, content, loadFiles, userId]);

  // 파일 열기 (URL 변경)
  const openFile = useCallback((fp) => {
    navigate(`/${userId}/${fp}`);
  }, [userId, navigate]);

  // 자동 저장
  const doSave = useCallback(async (fp, newContent) => {
    setSaveStatus('saving');
    try {
      const res = await fetch(`${API}/${userId}/file/${encodePath(fp)}`, {
        method: 'PUT',
        headers: authHeaders(),
        body: JSON.stringify({ content: newContent })
      });
      const data = await res.json();
      if (data.saved) {
        setSavedContent(newContent);
        setSaveStatus('saved');
        updateFileMeta(userId, fp, {
          size: new Blob([newContent]).size,
          hash: simpleHash(newContent)
        }).catch(err => console.error('Firebase meta update failed:', err));
        setTimeout(() => setSaveStatus(s => s === 'saved' ? 'idle' : s), 2000);
      }
    } catch (err) {
      console.error('Failed to save:', err);
      setSaveStatus('error');
    }
  }, [userId]);

  const handleChange = useCallback((val) => {
    setContent(val);
    if (saveTimer.current) clearTimeout(saveTimer.current);
    if (val !== savedContent && currentFile) {
      setSaveStatus('editing');
      saveTimer.current = setTimeout(() => {
        doSave(currentFile.path, val);
      }, AUTO_SAVE_DELAY);
    }
  }, [savedContent, currentFile, doSave]);

  useEffect(() => {
    return () => { if (saveTimer.current) clearTimeout(saveTimer.current); };
  }, []);

  // 파일 트리에서 경로 존재 여부 확인
  const pathExists = useCallback((targetPath, items) => {
    for (const item of items) {
      if (item.path === targetPath) return true;
      if (item.children && pathExists(targetPath, item.children)) return true;
    }
    return false;
  }, []);

  // 컨텍스트 메뉴 액션
  const handleNewFile = async (folderPath) => {
    const name = prompt('새 파일 이름 (.md 자동 추가)');
    if (!name) return;
    const fileName = name.endsWith('.md') ? name : `${name}.md`;
    const fp = folderPath ? `${folderPath}/${fileName}` : fileName;
    if (pathExists(fp, files)) {
      addToast(`📄 "${fileName}" — 같은 이름의 파일이 이미 존재합니다`, 'error', 3000);
      return;
    }
    const tid = addToast(`📄 "${fileName}" 생성 중...`, 'loading');
    setSidebarLoading(true);
    try {
      await fetch(`${API}/${userId}/file/${encodePath(fp)}`, {
        method: 'PUT',
        headers: authHeaders(),
        body: JSON.stringify({ content: `# ${name.replace('.md', '')}\n\n` })
      });
      await loadFiles();
      updateToast(tid, `📄 "${fileName}" 생성 완료!`, 'success');
      openFile(fp);
    } catch (err) {
      console.error('Failed to create file:', err);
      updateToast(tid, `📄 "${fileName}" 생성 실패`, 'error');
    } finally {
      setSidebarLoading(false);
    }
  };

  const handleGenerateSamples = async () => {
    if (!confirm('샘플 폴더와 파일을 생성할까요?')) return;
    const tid = addToast('🎲 샘플 생성 중...', 'loading');
    setSidebarLoading(true);
    const samples = [
      { path: 'Getting Started/welcome.md', content: '# Welcome to MDFlare! 🔥\n\nThis is your markdown workspace.\n\n## Quick Tips\n- Click any file to edit\n- Auto-saves after 1 second\n- Right-click for more options\n' },
      { path: 'Getting Started/markdown-guide.md', content: '# Markdown Guide\n\n## Headers\n# H1\n## H2\n### H3\n\n## Formatting\n**bold** *italic* ~~strikethrough~~\n\n## Lists\n- Item 1\n- Item 2\n  - Nested\n\n## Code\n```js\nconsole.log("Hello MDFlare!");\n```\n\n## Links\n[MDFlare](https://mdflare.com)\n' },
      { path: 'Notes/ideas.md', content: '# 💡 Ideas\n\n- [ ] Build something awesome\n- [ ] Share with the world\n- [x] Try MDFlare\n' },
      { path: 'Notes/meeting-notes.md', content: '# 📝 Meeting Notes\n\n## 2025-01-15\n- Discussed project roadmap\n- Next milestone: v1.0 launch\n- Action items:\n  1. Finalize design\n  2. Write documentation\n' },
      { path: 'Projects/project-alpha.md', content: '# Project Alpha 🚀\n\n## Overview\nA brief description of the project.\n\n## Status\n| Task | Status |\n|------|--------|\n| Design | ✅ Done |\n| Backend | 🔄 In Progress |\n| Frontend | 📋 Todo |\n\n## Notes\nKeep track of important decisions here.\n' },
      { path: 'journal.md', content: '# 📔 Journal\n\n## Today\nStarted using MDFlare for my notes.\nLove the clean interface and auto-save!\n\n---\n\n> "The best time to start writing is now."\n' },
    ];
    try {
      for (const s of samples) {
        await fetch(`${API}/${userId}/file/${encodePath(s.path)}`, {
          method: 'PUT',
          headers: authHeaders(),
          body: JSON.stringify({ content: s.content })
        });
      }
      await loadFiles();
      updateToast(tid, '🎲 샘플 생성 완료! (3폴더 + 6파일)', 'success', 3000);
    } catch (err) {
      console.error('Failed to generate samples:', err);
      updateToast(tid, '🎲 샘플 생성 실패', 'error');
    } finally {
      setSidebarLoading(false);
    }
  };

  const handleNewFolder = async (parentPath) => {
    const name = prompt('새 폴더 이름');
    if (!name) return;
    const folderFullPath = parentPath ? `${parentPath}/${name}` : name;
    if (pathExists(folderFullPath, files)) {
      addToast(`📁 "${name}" — 같은 이름의 폴더가 이미 존재합니다`, 'error', 3000);
      return;
    }
    const fp = `${folderFullPath}/.gitkeep`;
    const tid = addToast(`📁 "${name}" 폴더 생성 중...`, 'loading');
    setSidebarLoading(true);
    try {
      await fetch(`${API}/${userId}/file/${encodePath(fp)}`, {
        method: 'PUT',
        headers: authHeaders(),
        body: JSON.stringify({ content: '' })
      });
      await loadFiles();
      updateToast(tid, `📁 "${name}" 폴더 생성 완료!`, 'success');
    } catch (err) {
      console.error('Failed to create folder:', err);
      updateToast(tid, `📁 "${name}" 폴더 생성 실패`, 'error');
    } finally {
      setSidebarLoading(false);
    }
  };

  const handleRename = async (oldPath) => {
    const oldName = oldPath.split('/').pop();
    const newName = prompt('새 이름', oldName);
    if (!newName || newName === oldName) return;
    const parentPath = oldPath.includes('/') ? oldPath.substring(0, oldPath.lastIndexOf('/')) : '';
    const newPath = parentPath ? `${parentPath}/${newName}` : newName;
    const tid = addToast(`✏️ "${oldName}" → "${newName}" 변경 중...`, 'loading');
    setSidebarLoading(true);
    try {
      await fetch(`${API}/${userId}/rename`, {
        method: 'POST',
        headers: authHeaders(),
        body: JSON.stringify({ oldPath, newPath })
      });
      await loadFiles();
      updateToast(tid, `✏️ 이름 변경 완료!`, 'success');
      if (currentFile?.path === oldPath) openFile(newPath);
    } catch (err) {
      console.error('Failed to rename:', err);
      updateToast(tid, `✏️ 이름 변경 실패`, 'error');
    } finally {
      setSidebarLoading(false);
    }
  };

  const handleDelete = async (fp, name, type = 'file') => {
    const isFolder = type === 'folder';
    const label = isFolder ? '폴더' : '파일';
    if (!confirm(`"${name}" ${label}를 삭제할까요?${isFolder ? '\n(폴더 안의 모든 파일이 삭제됩니다)' : ''}`)) return;
    const tid = addToast(`🗑️ "${name}" ${label} 삭제 중...`, 'loading');
    setSidebarLoading(true);
    try {
      const folderQuery = isFolder ? '?folder=true' : '';
      await fetch(`${API}/${userId}/file/${encodePath(fp)}${folderQuery}`, { method: 'DELETE', headers: authHeaders() });
      await loadFiles();
      updateToast(tid, `🗑️ "${name}" ${label} 삭제 완료`, 'success');
      if (currentFile?.path === fp || (isFolder && currentFile?.path?.startsWith(fp + '/'))) {
        navigate(`/${userId}`);
      }
    } catch (err) {
      console.error('Failed to delete:', err);
      updateToast(tid, `🗑️ "${name}" ${label} 삭제 실패`, 'error');
    } finally {
      setSidebarLoading(false);
    }
  };

  const handleDuplicate = async (fp) => {
    const fileName = fp.split('/').pop();
    const tid = addToast(`📋 "${fileName}" 복제 중...`, 'loading');
    setSidebarLoading(true);
    try {
      const res = await fetch(`${API}/${userId}/file/${encodePath(fp)}`);
      const data = await res.json();
      const ext = fp.lastIndexOf('.md');
      const newPath = ext > 0 ? `${fp.slice(0, ext)} (copy).md` : `${fp} (copy)`;
      await fetch(`${API}/${userId}/file/${encodePath(newPath)}`, {
        method: 'PUT',
        headers: authHeaders(),
        body: JSON.stringify({ content: data.content })
      });
      await loadFiles();
      updateToast(tid, `📋 "${fileName}" 복제 완료!`, 'success');
    } catch (err) {
      console.error('Failed to duplicate:', err);
      updateToast(tid, `📋 복제 실패`, 'error');
    } finally {
      setSidebarLoading(false);
    }
  };

  const showContextMenu = (e, type, path, name) => {
    e.preventDefault();
    e.stopPropagation();
    setContextMenu({ x: e.clientX, y: e.clientY, type, path, name });
  };

  const handleLogout = async () => {
    await logout();
    navigate('/');
  };

  // API 토큰 발급
  const handleGenerateToken = async () => {
    if (!user) return;
    if (!confirm('API 토큰을 생성하시겠습니까?\n기존 토큰은 무효화됩니다.')) return;
    try {
      const res = await fetch('/api/token/generate', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ uid: user.uid, username: userId })
      });
      const data = await res.json();
      if (data.token) {
        prompt('API 토큰이 생성되었습니다.\n에이전트 앱에 입력하세요:', data.token);
      } else {
        alert('토큰 생성 실패: ' + (data.error || '알 수 없는 오류'));
      }
    } catch (err) {
      alert('토큰 생성 실패');
    }
  };

  const statusText = { idle: '', editing: '✏️', saving: '저장 중...', saved: '✓ 저장됨', error: '⚠️ 저장 실패' };
  const statusClass = { idle: '', editing: 'unsaved', saving: 'saving', saved: 'saved', error: 'error' };

  return (
    <>
      <header className="header">
        <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
          <button className="sidebar-toggle" onClick={() => setSidebarOpen(!sidebarOpen)}>
            {sidebarOpen ? '✕' : '☰'}
          </button>
          <h1 onClick={() => navigate(`/${userId}`)} style={{ cursor: 'pointer' }}>🔥 MDFlare</h1>
        </div>
        <div style={{ display: 'flex', alignItems: 'center', gap: 12 }}>
          <span className="user-badge">👤 {user?.displayName || userId}</span>
          <button className="logout-btn" onClick={handleGenerateToken}>🔑 API 토큰</button>
          <button className="logout-btn" onClick={handleLogout}>로그아웃</button>
        </div>
      </header>

      <div className="main">
        <aside className={`sidebar ${sidebarOpen ? 'open' : ''}`}>
          <div className="sidebar-header" onContextMenu={(e) => { e.preventDefault(); showContextMenu(e, 'root', '', 'root'); }}>
            <div style={{ display: 'flex', flexDirection: 'column', gap: 2, flex: 1, minWidth: 0 }}>
              <span>📁 Files {sidebarLoading && <span className="sidebar-spinner">⟳</span>}</span>
              {focusedFolder && (
                <span className="focused-folder-label" onClick={() => setFocusedFolder('')}>
                  📂 {focusedFolder} ✕
                </span>
              )}
            </div>
            <div className="sidebar-actions">
              <button className="sidebar-action-btn" onClick={() => handleNewFile(focusedFolder)} title={focusedFolder ? `${focusedFolder}에 새 파일` : '새 파일'} disabled={sidebarLoading}>📄+</button>
              <button className="sidebar-action-btn" onClick={() => handleNewFolder(focusedFolder)} title={focusedFolder ? `${focusedFolder}에 새 폴더` : '새 폴더'} disabled={sidebarLoading}>📁+</button>
            </div>
          </div>
          <div className="file-tree" onContextMenu={(e) => {
            e.preventDefault();
            if (e.target.closest('.tree-item')) return;
            showContextMenu(e, 'root', '', 'root');
          }}>
            <FileTree items={files} currentPath={currentFile?.path} onSelect={openFile} onContextMenu={showContextMenu} focusedFolder={focusedFolder} onFocusFolder={setFocusedFolder} onNewFile={handleNewFile} />
          </div>
          <div className="sidebar-footer">
            <span title={__LAST_CHANGE__}>v{__BUILD_VERSION__} · {__LAST_CHANGE__}</span>
            <button className="sample-btn" onClick={handleGenerateSamples} disabled={sidebarLoading}>🎲 샘플</button>
          </div>
          <div className="sidebar-handle" onClick={() => setSidebarOpen(false)}>
            ▲
          </div>
        </aside>

        <div className="editor-area">
          {currentFile ? (
            currentFile.loading ? (
              <div className="empty-state">
                <div className="loading-spinner">⟳</div>
                <p>불러오는 중...</p>
              </div>
            ) : (
            <>
              <div className="editor-toolbar">
                <span className="file-path">{currentFile.path}</span>
                <div style={{ display: 'flex', gap: 8, alignItems: 'center' }}>
                  <div className="tab-bar">
                    <button className={`tab-btn ${view === 'edit' ? 'active' : ''}`} onClick={() => setView('edit')}>Edit</button>
                    <button className={`tab-btn ${view === 'split' ? 'active' : ''}`} onClick={() => setView('split')}>Split</button>
                    <button className={`tab-btn ${view === 'preview' ? 'active' : ''}`} onClick={() => setView('preview')}>Preview</button>
                  </div>
                  <span className={`save-status ${statusClass[saveStatus]}`}>{statusText[saveStatus]}</span>
                </div>
              </div>
              <div className="editor-content">
                {(view === 'edit' || view === 'split') && (
                  <CodeMirror value={content} onChange={handleChange}
                    extensions={[markdown(), darkTheme, EditorView.lineWrapping]}
                    theme="none" style={{ flex: 1, overflow: 'auto' }} />
                )}
                {(view === 'preview' || view === 'split') && (
                  <div className="preview">
                    <ReactMarkdown remarkPlugins={[remarkGfm]}>{content}</ReactMarkdown>
                  </div>
                )}
              </div>
            </>
            )
          ) : (
            <div className="empty-state">
              <div className="logo">🔥</div>
              <p>파일을 선택하세요</p>
            </div>
          )}
        </div>
      </div>

      {contextMenu && (
        <ContextMenu {...contextMenu}
          onNewFile={handleNewFile} onNewFolder={handleNewFolder}
          onRename={handleRename} onDelete={handleDelete}
          onDuplicate={handleDuplicate} onClose={() => setContextMenu(null)} />
      )}

      <Toast toasts={toasts} onRemove={removeToast} />
    </>
  );
}

function ContextMenu({ x, y, type, path, name, onNewFile, onNewFolder, onRename, onDelete, onDuplicate, onClose }) {
  const menuRef = useRef(null);
  useEffect(() => {
    if (menuRef.current) {
      const rect = menuRef.current.getBoundingClientRect();
      if (rect.right > window.innerWidth) menuRef.current.style.left = `${window.innerWidth - rect.width - 8}px`;
      if (rect.bottom > window.innerHeight) menuRef.current.style.top = `${window.innerHeight - rect.height - 8}px`;
    }
  }, []);

  const folderPath = type === 'folder' ? path : type === 'root' ? '' : path.substring(0, path.lastIndexOf('/'));

  return (
    <div className="context-menu" ref={menuRef} style={{ left: x, top: y }}>
      <div className="context-item" onClick={() => { onNewFile(folderPath); onClose(); }}>📄 새 파일</div>
      <div className="context-item" onClick={() => { onNewFolder(folderPath); onClose(); }}>📁 새 폴더</div>
      {type !== 'root' && (
        <>
          <div className="context-divider" />
          <div className="context-item" onClick={() => { onRename(path, type); onClose(); }}>✏️ 이름 변경</div>
          {type === 'file' && (
            <div className="context-item" onClick={() => { onDuplicate(path); onClose(); }}>📋 복제</div>
          )}
          <div className="context-divider" />
          <div className="context-item danger" onClick={() => { onDelete(path, name, type); onClose(); }}>🗑️ 삭제</div>
        </>
      )}
    </div>
  );
}

// 롱프레스 훅 (모바일 터치 + 데스크탑 클릭/우클릭 모두 지원)
function useLongPress(onLongPress, onClick, ms = 500) {
  const timerRef = useRef(null);
  const movedRef = useRef(false);
  const triggeredRef = useRef(false);
  const touchFiredRef = useRef(false);

  const start = useCallback((e) => {
    movedRef.current = false;
    triggeredRef.current = false;
    touchFiredRef.current = true;
    const touch = e.touches?.[0];
    const x = touch?.clientX ?? e.clientX;
    const y = touch?.clientY ?? e.clientY;
    timerRef.current = setTimeout(() => {
      triggeredRef.current = true;
      if (navigator.vibrate) navigator.vibrate(30);
      onLongPress({ clientX: x, clientY: y, preventDefault: () => {}, stopPropagation: () => {} });
    }, ms);
  }, [onLongPress, ms]);

  const move = useCallback(() => {
    movedRef.current = true;
    if (timerRef.current) { clearTimeout(timerRef.current); timerRef.current = null; }
  }, []);

  const end = useCallback((e) => {
    if (timerRef.current) { clearTimeout(timerRef.current); timerRef.current = null; }
    if (triggeredRef.current) {
      e.preventDefault();
      return;
    }
    if (!movedRef.current && onClick) onClick(e);
    // 터치 후 브라우저가 click도 발생시키므로 잠시 플래그 유지
    setTimeout(() => { touchFiredRef.current = false; }, 300);
  }, [onClick]);

  // 데스크탑 클릭 핸들러 (터치 직후 발생하는 synthetic click은 무시)
  const handleClick = useCallback((e) => {
    if (touchFiredRef.current) return;
    if (onClick) onClick(e);
  }, [onClick]);

  return {
    onTouchStart: start,
    onTouchMove: move,
    onTouchEnd: end,
    onClick: handleClick,
    onContextMenu: (e) => { e.preventDefault(); e.stopPropagation(); onLongPress(e); },
  };
}

function FileTree({ items, currentPath, onSelect, onContextMenu, focusedFolder, onFocusFolder, onNewFile, depth = 0 }) {
  return items.map((item) => (
    <div key={item.path}>
      {item.type === 'folder' ? (
        <FolderItem item={item} currentPath={currentPath} onSelect={onSelect} onContextMenu={onContextMenu} focusedFolder={focusedFolder} onFocusFolder={onFocusFolder} onNewFile={onNewFile} depth={depth} />
      ) : (
        <FileItem item={item} currentPath={currentPath} onSelect={onSelect} onContextMenu={onContextMenu} depth={depth} />
      )}
    </div>
  ));
}

function FileItem({ item, currentPath, onSelect, onContextMenu, depth }) {
  const longPressHandlers = useLongPress(
    (e) => onContextMenu(e, 'file', item.path, item.name),
    () => onSelect(item.path),
  );
  return (
    <div className={`tree-item ${item.path === currentPath ? 'active' : ''}`}
      style={{ paddingLeft: 16 + depth * 16 }}
      {...longPressHandlers}>
      <span className="icon">📄</span>{item.name}
    </div>
  );
}

function FolderItem({ item, currentPath, onSelect, onContextMenu, focusedFolder, onFocusFolder, onNewFile, depth }) {
  const [open, setOpen] = useState(true);
  const isFocused = focusedFolder === item.path;
  // .gitkeep만 있는 폴더는 빈 폴더로 취급
  const visibleChildren = item.children?.filter(c => c.name !== '.gitkeep') || [];
  const isEmpty = visibleChildren.length === 0;
  const longPressHandlers = useLongPress(
    (e) => onContextMenu(e, 'folder', item.path, item.name),
    () => {
      setOpen(!open);
      if (onFocusFolder) onFocusFolder(isFocused ? '' : item.path);
    },
  );
  return (
    <>
      <div className={`tree-item tree-folder ${isFocused ? 'focused' : ''}`} style={{ paddingLeft: 16 + depth * 16 }}
        {...longPressHandlers}>
        <span className="icon">{open ? '📂' : '📁'}</span>{item.name}
      </div>
      {open && (
        <div style={{ paddingLeft: 0 }}>
          {isEmpty ? (
            <div className="empty-folder" style={{ paddingLeft: 32 + depth * 16 }}>
              <button className="empty-folder-btn" onClick={() => onNewFile && onNewFile(item.path)}>
                + 새 파일
              </button>
            </div>
          ) : (
            <FileTree items={visibleChildren} currentPath={currentPath} onSelect={onSelect} onContextMenu={onContextMenu} focusedFolder={focusedFolder} onFocusFolder={onFocusFolder} onNewFile={onNewFile} depth={depth + 1} />
          )}
        </div>
      )}
    </>
  );
}
