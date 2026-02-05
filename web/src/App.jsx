import React, { useState, useEffect, useCallback, useRef } from 'react';
import CodeMirror from '@uiw/react-codemirror';
import { markdown } from '@codemirror/lang-markdown';
import { EditorView } from '@codemirror/view';
import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';

const USER_ID = 'test';
const API = '/api';
const AUTO_SAVE_DELAY = 1000;

const darkTheme = EditorView.theme({
  '&': { backgroundColor: '#0d1117', color: '#e6edf3' },
  '.cm-content': { caretColor: '#58a6ff' },
  '.cm-cursor': { borderLeftColor: '#58a6ff' },
  '&.cm-focused .cm-selectionBackground, .cm-selectionBackground': { backgroundColor: '#1f6feb44' },
  '.cm-gutters': { backgroundColor: '#161b22', color: '#484f58', border: 'none' },
  '.cm-activeLineGutter': { backgroundColor: '#1f6feb22' },
  '.cm-activeLine': { backgroundColor: '#1f6feb11' },
}, { dark: true });

export default function App() {
  const [files, setFiles] = useState([]);
  const [currentFile, setCurrentFile] = useState(null);
  const [content, setContent] = useState('');
  const [savedContent, setSavedContent] = useState('');
  const [view, setView] = useState('edit');
  const [saveStatus, setSaveStatus] = useState('idle');
  const [contextMenu, setContextMenu] = useState(null);
  const saveTimer = useRef(null);

  const isUnsaved = content !== savedContent;

  // 파일 트리 로드
  const loadFiles = useCallback(() => {
    fetch(`${API}/${USER_ID}/files`)
      .then(r => r.json())
      .then(data => setFiles(data.files))
      .catch(err => console.error('Failed to load files:', err));
  }, []);

  useEffect(() => { loadFiles(); }, [loadFiles]);

  // 컨텍스트 메뉴 닫기
  useEffect(() => {
    const handler = () => setContextMenu(null);
    window.addEventListener('click', handler);
    return () => window.removeEventListener('click', handler);
  }, []);

  // 파일 열기
  const openFile = useCallback(async (filePath) => {
    if (saveTimer.current) {
      clearTimeout(saveTimer.current);
      saveTimer.current = null;
    }
    try {
      const res = await fetch(`${API}/${USER_ID}/file/${filePath}`);
      const data = await res.json();
      setCurrentFile(data);
      setContent(data.content);
      setSavedContent(data.content);
      setSaveStatus('idle');
    } catch (err) {
      console.error('Failed to open file:', err);
    }
  }, []);

  // 자동 저장
  const doSave = useCallback(async (filePath, newContent) => {
    setSaveStatus('saving');
    try {
      const res = await fetch(`${API}/${USER_ID}/file/${filePath}`, {
        method: 'PUT',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ content: newContent })
      });
      const data = await res.json();
      if (data.saved) {
        setSavedContent(newContent);
        setSaveStatus('saved');
        setTimeout(() => setSaveStatus(s => s === 'saved' ? 'idle' : s), 2000);
      }
    } catch (err) {
      console.error('Failed to save:', err);
      setSaveStatus('error');
    }
  }, []);

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

  // === 컨텍스트 메뉴 액션들 ===

  const handleNewFile = async (folderPath) => {
    const name = prompt('새 파일 이름 (.md 자동 추가)');
    if (!name) return;
    const fileName = name.endsWith('.md') ? name : `${name}.md`;
    const filePath = folderPath ? `${folderPath}/${fileName}` : fileName;
    try {
      await fetch(`${API}/${USER_ID}/file/${filePath}`, {
        method: 'PUT',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ content: `# ${name.replace('.md', '')}\n\n` })
      });
      loadFiles();
      openFile(filePath);
    } catch (err) {
      console.error('Failed to create file:', err);
    }
  };

  const handleNewFolder = async (parentPath) => {
    const name = prompt('새 폴더 이름');
    if (!name) return;
    const filePath = parentPath ? `${parentPath}/${name}/.gitkeep` : `${name}/.gitkeep`;
    try {
      await fetch(`${API}/${USER_ID}/file/${filePath}`, {
        method: 'PUT',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ content: '' })
      });
      loadFiles();
    } catch (err) {
      console.error('Failed to create folder:', err);
    }
  };

  const handleRename = async (oldPath, type) => {
    const oldName = oldPath.split('/').pop();
    const newName = prompt('새 이름', oldName);
    if (!newName || newName === oldName) return;
    const parentPath = oldPath.includes('/') ? oldPath.substring(0, oldPath.lastIndexOf('/')) : '';
    const newPath = parentPath ? `${parentPath}/${newName}` : newName;
    try {
      await fetch(`${API}/${USER_ID}/rename`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ oldPath, newPath })
      });
      loadFiles();
      if (currentFile?.path === oldPath) {
        openFile(newPath);
      }
    } catch (err) {
      console.error('Failed to rename:', err);
    }
  };

  const handleDelete = async (filePath, name) => {
    if (!confirm(`"${name}" 삭제할까요?`)) return;
    try {
      await fetch(`${API}/${USER_ID}/file/${filePath}`, { method: 'DELETE' });
      loadFiles();
      if (currentFile?.path === filePath) {
        setCurrentFile(null);
        setContent('');
        setSavedContent('');
      }
    } catch (err) {
      console.error('Failed to delete:', err);
    }
  };

  const handleDuplicate = async (filePath) => {
    try {
      const res = await fetch(`${API}/${USER_ID}/file/${filePath}`);
      const data = await res.json();
      const ext = filePath.lastIndexOf('.md');
      const newPath = ext > 0 ? `${filePath.slice(0, ext)} (copy).md` : `${filePath} (copy)`;
      await fetch(`${API}/${USER_ID}/file/${newPath}`, {
        method: 'PUT',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ content: data.content })
      });
      loadFiles();
    } catch (err) {
      console.error('Failed to duplicate:', err);
    }
  };

  const showContextMenu = (e, type, path, name) => {
    e.preventDefault();
    e.stopPropagation();
    setContextMenu({ x: e.clientX, y: e.clientY, type, path, name });
  };

  const statusText = { idle: '', editing: '✏️', saving: '저장 중...', saved: '✓ 저장됨', error: '⚠️ 저장 실패' };
  const statusClass = { idle: '', editing: 'unsaved', saving: 'saving', saved: 'saved', error: 'error' };

  return (
    <>
      <header className="header">
        <h1>🔥 MDFlare</h1>
        <span className="user-badge">👤 {USER_ID}</span>
      </header>

      <div className="main">
        <aside className="sidebar">
          <div className="sidebar-header"
            onContextMenu={(e) => showContextMenu(e, 'root', '', 'root')}
          >📁 Files</div>
          <div className="file-tree"
            onContextMenu={(e) => {
              if (e.target.closest('.tree-item')) return;
              showContextMenu(e, 'root', '', 'root');
            }}
          >
            <FileTree
              items={files}
              currentPath={currentFile?.path}
              onSelect={openFile}
              onContextMenu={showContextMenu}
            />
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
                  <CodeMirror
                    value={content}
                    onChange={handleChange}
                    extensions={[markdown(), darkTheme, EditorView.lineWrapping]}
                    theme="none"
                    style={{ flex: 1, overflow: 'auto' }}
                  />
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

      {/* 컨텍스트 메뉴 */}
      {contextMenu && (
        <ContextMenu
          {...contextMenu}
          onNewFile={handleNewFile}
          onNewFolder={handleNewFolder}
          onRename={handleRename}
          onDelete={handleDelete}
          onDuplicate={handleDuplicate}
          onClose={() => setContextMenu(null)}
        />
      )}
    </>
  );
}

// 컨텍스트 메뉴 컴포넌트
function ContextMenu({ x, y, type, path, name, onNewFile, onNewFolder, onRename, onDelete, onDuplicate, onClose }) {
  const menuRef = useRef(null);

  useEffect(() => {
    if (menuRef.current) {
      const rect = menuRef.current.getBoundingClientRect();
      if (rect.right > window.innerWidth) {
        menuRef.current.style.left = `${window.innerWidth - rect.width - 8}px`;
      }
      if (rect.bottom > window.innerHeight) {
        menuRef.current.style.top = `${window.innerHeight - rect.height - 8}px`;
      }
    }
  }, []);

  const folderPath = type === 'folder' ? path : type === 'root' ? '' : path.substring(0, path.lastIndexOf('/'));

  return (
    <div className="context-menu" ref={menuRef} style={{ left: x, top: y }}>
      <div className="context-item" onClick={() => { onNewFile(folderPath); onClose(); }}>
        📄 새 파일
      </div>
      <div className="context-item" onClick={() => { onNewFolder(folderPath); onClose(); }}>
        📁 새 폴더
      </div>
      {type !== 'root' && (
        <>
          <div className="context-divider" />
          <div className="context-item" onClick={() => { onRename(path, type); onClose(); }}>
            ✏️ 이름 변경
          </div>
          {type === 'file' && (
            <div className="context-item" onClick={() => { onDuplicate(path); onClose(); }}>
              📋 복제
            </div>
          )}
          <div className="context-divider" />
          <div className="context-item danger" onClick={() => { onDelete(path, name); onClose(); }}>
            🗑️ 삭제
          </div>
        </>
      )}
    </div>
  );
}

// 파일 트리 컴포넌트
function FileTree({ items, currentPath, onSelect, onContextMenu, depth = 0 }) {
  return items.map((item) => (
    <div key={item.path}>
      {item.type === 'folder' ? (
        <FolderItem item={item} currentPath={currentPath} onSelect={onSelect} onContextMenu={onContextMenu} depth={depth} />
      ) : (
        <div
          className={`tree-item ${item.path === currentPath ? 'active' : ''}`}
          style={{ paddingLeft: 16 + depth * 16 }}
          onClick={() => onSelect(item.path)}
          onContextMenu={(e) => onContextMenu(e, 'file', item.path, item.name)}
        >
          <span className="icon">📄</span>
          {item.name}
        </div>
      )}
    </div>
  ));
}

function FolderItem({ item, currentPath, onSelect, onContextMenu, depth }) {
  const [open, setOpen] = useState(true);

  return (
    <>
      <div
        className="tree-item tree-folder"
        style={{ paddingLeft: 16 + depth * 16 }}
        onClick={() => setOpen(!open)}
        onContextMenu={(e) => onContextMenu(e, 'folder', item.path, item.name)}
      >
        <span className="icon">{open ? '📂' : '📁'}</span>
        {item.name}
      </div>
      {open && item.children && (
        <div style={{ paddingLeft: 0 }}>
          <FileTree items={item.children} currentPath={currentPath} onSelect={onSelect} onContextMenu={onContextMenu} depth={depth + 1} />
        </div>
      )}
    </>
  );
}
