import React, { useState, useEffect, useCallback } from 'react';
import CodeMirror from '@uiw/react-codemirror';
import { markdown } from '@codemirror/lang-markdown';
import { EditorView } from '@codemirror/view';
import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';

const USER_ID = 'test';
const API = '/api';

// Dark theme for CodeMirror
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
  const [view, setView] = useState('split'); // edit | preview | split
  const [saving, setSaving] = useState(false);
  const [lastSaved, setLastSaved] = useState(null);

  const isUnsaved = content !== savedContent;

  // 파일 트리 로드
  useEffect(() => {
    fetch(`${API}/${USER_ID}/files`)
      .then(r => r.json())
      .then(data => setFiles(data.files))
      .catch(err => console.error('Failed to load files:', err));
  }, []);

  // 파일 열기
  const openFile = useCallback(async (filePath) => {
    try {
      const res = await fetch(`${API}/${USER_ID}/file/${filePath}`);
      const data = await res.json();
      setCurrentFile(data);
      setContent(data.content);
      setSavedContent(data.content);
      setLastSaved(null);
    } catch (err) {
      console.error('Failed to open file:', err);
    }
  }, []);

  // 파일 저장
  const saveFile = useCallback(async () => {
    if (!currentFile || !isUnsaved) return;
    setSaving(true);
    try {
      const res = await fetch(`${API}/${USER_ID}/file/${currentFile.path}`, {
        method: 'PUT',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ content })
      });
      const data = await res.json();
      if (data.saved) {
        setSavedContent(content);
        setLastSaved(new Date().toLocaleTimeString());
      }
    } catch (err) {
      console.error('Failed to save:', err);
    }
    setSaving(false);
  }, [currentFile, content, isUnsaved]);

  // Ctrl+S / Cmd+S 저장
  useEffect(() => {
    const handler = (e) => {
      if ((e.metaKey || e.ctrlKey) && e.key === 's') {
        e.preventDefault();
        saveFile();
      }
    };
    window.addEventListener('keydown', handler);
    return () => window.removeEventListener('keydown', handler);
  }, [saveFile]);

  return (
    <>
      <header className="header">
        <h1>🔥 MDFlare</h1>
        <span className="user-badge">👤 {USER_ID}</span>
      </header>

      <div className="main">
        {/* Sidebar */}
        <aside className="sidebar">
          <div className="sidebar-header">📁 Files</div>
          <div className="file-tree">
            <FileTree
              items={files}
              currentPath={currentFile?.path}
              onSelect={openFile}
            />
          </div>
        </aside>

        {/* Editor */}
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
                  <span className={`save-status ${isUnsaved ? 'unsaved' : ''}`}>
                    {isUnsaved ? '● 수정됨' : lastSaved ? `✓ ${lastSaved}` : ''}
                  </span>
                  <button className="save-btn" onClick={saveFile} disabled={!isUnsaved || saving}>
                    {saving ? '저장중...' : '저장'}
                  </button>
                </div>
              </div>

              <div className="editor-content">
                {(view === 'edit' || view === 'split') && (
                  <CodeMirror
                    value={content}
                    onChange={(val) => setContent(val)}
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
    </>
  );
}

// 파일 트리 컴포넌트
function FileTree({ items, currentPath, onSelect, depth = 0 }) {
  return items.map((item) => (
    <div key={item.path}>
      {item.type === 'folder' ? (
        <FolderItem item={item} currentPath={currentPath} onSelect={onSelect} depth={depth} />
      ) : (
        <div
          className={`tree-item ${item.path === currentPath ? 'active' : ''}`}
          style={{ paddingLeft: 16 + depth * 16 }}
          onClick={() => onSelect(item.path)}
        >
          <span className="icon">📄</span>
          {item.name}
        </div>
      )}
    </div>
  ));
}

function FolderItem({ item, currentPath, onSelect, depth }) {
  const [open, setOpen] = useState(true);

  return (
    <>
      <div
        className="tree-item tree-folder"
        style={{ paddingLeft: 16 + depth * 16 }}
        onClick={() => setOpen(!open)}
      >
        <span className="icon">{open ? '📂' : '📁'}</span>
        {item.name}
      </div>
      {open && item.children && (
        <div className="tree-folder-children" style={{ paddingLeft: 0 }}>
          <FileTree items={item.children} currentPath={currentPath} onSelect={onSelect} depth={depth + 1} />
        </div>
      )}
    </>
  );
}
