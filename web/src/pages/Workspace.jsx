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
  const saveTimer = useRef(null);

  // 파일 트리 로드
  const loadFiles = useCallback(() => {
    fetch(`${API}/${userId}/files`)
      .then(r => r.json())
      .then(data => setFiles(data.files || []))
      .catch(err => console.error('Failed to load files:', err));
  }, [userId]);

  useEffect(() => { loadFiles(); }, [loadFiles]);

  // URL 경로에서 파일 열기
  useEffect(() => {
    if (filePath) {
      const fp = filePath;
      fetch(`${API}/${userId}/file/${fp}`)
        .then(r => r.json())
        .then(data => {
          if (!data.error) {
            setCurrentFile(data);
            setContent(data.content);
            setSavedContent(data.content);
            setSaveStatus('idle');
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
          fetch(`${API}/${userId}/file/${currentFile.path}`)
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
    setSidebarOpen(false);
  }, [userId, navigate]);

  // 자동 저장
  const doSave = useCallback(async (fp, newContent) => {
    setSaveStatus('saving');
    try {
      const res = await fetch(`${API}/${userId}/file/${fp}`, {
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

  // 컨텍스트 메뉴 액션
  const handleNewFile = async (folderPath) => {
    const name = prompt('새 파일 이름 (.md 자동 추가)');
    if (!name) return;
    const fileName = name.endsWith('.md') ? name : `${name}.md`;
    const fp = folderPath ? `${folderPath}/${fileName}` : fileName;
    try {
      await fetch(`${API}/${userId}/file/${fp}`, {
        method: 'PUT',
        headers: authHeaders(),
        body: JSON.stringify({ content: `# ${name.replace('.md', '')}\n\n` })
      });
      loadFiles();
      openFile(fp);
    } catch (err) { console.error('Failed to create file:', err); }
  };

  const handleNewFolder = async (parentPath) => {
    const name = prompt('새 폴더 이름');
    if (!name) return;
    const fp = parentPath ? `${parentPath}/${name}/.gitkeep` : `${name}/.gitkeep`;
    try {
      await fetch(`${API}/${userId}/file/${fp}`, {
        method: 'PUT',
        headers: authHeaders(),
        body: JSON.stringify({ content: '' })
      });
      loadFiles();
    } catch (err) { console.error('Failed to create folder:', err); }
  };

  const handleRename = async (oldPath) => {
    const oldName = oldPath.split('/').pop();
    const newName = prompt('새 이름', oldName);
    if (!newName || newName === oldName) return;
    const parentPath = oldPath.includes('/') ? oldPath.substring(0, oldPath.lastIndexOf('/')) : '';
    const newPath = parentPath ? `${parentPath}/${newName}` : newName;
    try {
      await fetch(`${API}/${userId}/rename`, {
        method: 'POST',
        headers: authHeaders(),
        body: JSON.stringify({ oldPath, newPath })
      });
      loadFiles();
      if (currentFile?.path === oldPath) openFile(newPath);
    } catch (err) { console.error('Failed to rename:', err); }
  };

  const handleDelete = async (fp, name) => {
    if (!confirm(`"${name}" 삭제할까요?`)) return;
    try {
      await fetch(`${API}/${userId}/file/${fp}`, { method: 'DELETE', headers: authHeaders() });
      loadFiles();
      if (currentFile?.path === fp) navigate(`/${userId}`);
    } catch (err) { console.error('Failed to delete:', err); }
  };

  const handleDuplicate = async (fp) => {
    try {
      const res = await fetch(`${API}/${userId}/file/${fp}`);
      const data = await res.json();
      const ext = fp.lastIndexOf('.md');
      const newPath = ext > 0 ? `${fp.slice(0, ext)} (copy).md` : `${fp} (copy)`;
      await fetch(`${API}/${userId}/file/${newPath}`, {
        method: 'PUT',
        headers: authHeaders(),
        body: JSON.stringify({ content: data.content })
      });
      loadFiles();
    } catch (err) { console.error('Failed to duplicate:', err); }
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
          <div className="sidebar-header" onContextMenu={(e) => showContextMenu(e, 'root', '', 'root')}>
            📁 Files
          </div>
          <div className="file-tree" onContextMenu={(e) => {
            if (e.target.closest('.tree-item')) return;
            showContextMenu(e, 'root', '', 'root');
          }}>
            <FileTree items={files} currentPath={currentFile?.path} onSelect={openFile} onContextMenu={showContextMenu} />
          </div>
        </aside>

        <div className="editor-area">
          {currentFile ? (
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
          <div className="context-item danger" onClick={() => { onDelete(path, name); onClose(); }}>🗑️ 삭제</div>
        </>
      )}
    </div>
  );
}

function FileTree({ items, currentPath, onSelect, onContextMenu, depth = 0 }) {
  return items.map((item) => (
    <div key={item.path}>
      {item.type === 'folder' ? (
        <FolderItem item={item} currentPath={currentPath} onSelect={onSelect} onContextMenu={onContextMenu} depth={depth} />
      ) : (
        <div className={`tree-item ${item.path === currentPath ? 'active' : ''}`}
          style={{ paddingLeft: 16 + depth * 16 }}
          onClick={() => onSelect(item.path)}
          onContextMenu={(e) => onContextMenu(e, 'file', item.path, item.name)}>
          <span className="icon">📄</span>{item.name}
        </div>
      )}
    </div>
  ));
}

function FolderItem({ item, currentPath, onSelect, onContextMenu, depth }) {
  const [open, setOpen] = useState(true);
  return (
    <>
      <div className="tree-item tree-folder" style={{ paddingLeft: 16 + depth * 16 }}
        onClick={() => setOpen(!open)}
        onContextMenu={(e) => onContextMenu(e, 'folder', item.path, item.name)}>
        <span className="icon">{open ? '📂' : '📁'}</span>{item.name}
      </div>
      {open && item.children && (
        <div style={{ paddingLeft: 0 }}>
          <FileTree items={item.children} currentPath={currentPath} onSelect={onSelect} onContextMenu={onContextMenu} depth={depth + 1} />
        </div>
      )}
    </>
  );
}
