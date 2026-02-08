import React, { useState, useEffect, useCallback, useRef, useMemo } from 'react';
import { useParams, useNavigate } from 'react-router-dom';
import CodeMirror from '@uiw/react-codemirror';
import { markdown } from '@codemirror/lang-markdown';
import { EditorView } from '@codemirror/view';
import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';
import { updateFileMeta, deleteFileMeta, onFilesChanged, simpleHash, computeLineDiff, logout, auth } from '../firebase';
import { getAppName } from '../components/AppTitle';
import AgentStatus from '../components/AgentStatus';

const API = '/api';
const AUTO_SAVE_DELAY = 1000;

// Private Vault 설정 가져오기
function getPrivateVaultConfig() {
  return {
    serverUrl: localStorage.getItem('mdflare_server_url') || 'http://localhost:7779',
    token: localStorage.getItem('mdflare_token') || '',
    useProxy: localStorage.getItem('mdflare_use_proxy') === 'true',
  };
}

// Private Vault API URL 생성 (프록시 지원)
function buildPrivateVaultUrl(path) {
  const { serverUrl, useProxy } = getPrivateVaultConfig();
  if (useProxy) {
    const server = serverUrl.replace('http://', '').replace('https://', '');
    return `/_tunnel?server=${encodeURIComponent(server)}&path=${encodeURIComponent(path)}`;
  }
  return `${serverUrl}${path}`;
}

// API 경로 인코딩 헬퍼 (한글 등 유니코드 지원, / 유지)
const encodePath = (p) => p.split('/').map(s => encodeURIComponent(s)).join('/');

// 인증 헤더 생성 헬퍼 (비동기 - ID Token 사용)
async function authHeaders(isPrivateVault = false) {
  const headers = { 'Content-Type': 'application/json' };
  
  if (isPrivateVault) {
    const { token } = getPrivateVaultConfig();
    if (token) {
      headers['Authorization'] = `Bearer ${token}`;
    }
  } else if (auth.currentUser) {
    try {
      const idToken = await auth.currentUser.getIdToken();
      headers['Authorization'] = `Bearer ${idToken}`;
    } catch (e) {
      console.error('Failed to get ID token:', e);
    }
  }
  return headers;
}

// API base URL 가져오기 (Private Vault는 프록시 지원)
function getApiBase(isPrivateVault = false) {
  if (isPrivateVault) {
    const { serverUrl, useProxy } = getPrivateVaultConfig();
    if (useProxy) {
      // 프록시 사용 시 빈 문자열 반환, buildPrivateVaultUrl 사용
      return '__PROXY__';
    }
    return serverUrl;
  }
  return '';
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

const lightTheme = EditorView.theme({
  '&': { backgroundColor: '#ffffff', color: '#24292f' },
  '.cm-content': { caretColor: '#0969da' },
  '.cm-cursor': { borderLeftColor: '#0969da' },
  '&.cm-focused .cm-selectionBackground, .cm-selectionBackground': { backgroundColor: '#0969da22' },
  '.cm-gutters': { backgroundColor: '#f6f8fa', color: '#8c959f', border: 'none' },
  '.cm-activeLineGutter': { backgroundColor: '#0969da11' },
  '.cm-activeLine': { backgroundColor: '#0969da08' },
}, { dark: false });

const cmStyle = { flex: 1, overflow: 'auto' };

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

export default function Workspace({ user, isPrivateVault = false }) {
  const { userId: paramUserId, '*': filePath } = useParams();
  const navigate = useNavigate();
  
  // Private Vault 모드에서는 userId가 필요 없음
  const userId = isPrivateVault ? '' : paramUserId;
  const pvConfig = isPrivateVault ? getPrivateVaultConfig() : null;
  
  // API URL 생성 함수
  const buildApiUrl = (path) => {
    if (isPrivateVault && pvConfig) {
      if (pvConfig.useProxy) {
        const server = pvConfig.serverUrl.replace('http://', '').replace('https://', '');
        return `/_tunnel?server=${encodeURIComponent(server)}&path=${encodeURIComponent('/api' + path)}`;
      }
      return `${pvConfig.serverUrl}/api${path}`;
    }
    return `${API}/${userId}${path}`;
  };

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
  const [dragOver, setDragOver] = useState(null);
  const [dragSrc, setDragSrc] = useState(null);
  const [searchQuery, setSearchQuery] = useState('');
  const [lightMode, setLightMode] = useState(() => localStorage.getItem('mdflare-theme') === 'light');
  const [recentFiles, setRecentFiles] = useState(() => {
    try { return JSON.parse(localStorage.getItem('mdflare-recent') || '[]'); } catch { return []; }
  });
  const saveTimer = useRef(null);
  const toastId = useRef(0);
  const contentRef = useRef('');
  const savedContentRef = useRef('');
  const lastSavedHashRef = useRef(null);

  // refs를 state와 동기화 (RTDB 리스너에서 최신 값 참조용)
  useEffect(() => { contentRef.current = content; }, [content]);
  useEffect(() => { savedContentRef.current = savedContent; }, [savedContent]);

  // CodeMirror extensions 메모이제이션 (리렌더 시 에디터 재설정 방지)
  const cmExtensions = useMemo(
    () => [markdown(), lightMode ? lightTheme : darkTheme, EditorView.lineWrapping],
    [lightMode]
  );

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

  // 파일 트리 정렬 (폴더 먼저, 이름순)
  const sortFiles = useCallback((items) => {
    return [...items].sort((a, b) => {
      if (a.type === 'folder' && b.type !== 'folder') return -1;
      if (a.type !== 'folder' && b.type === 'folder') return 1;
      return a.name.localeCompare(b.name, 'ko');
    }).map(item => item.children ? { ...item, children: sortFiles(item.children) } : item);
  }, []);

  // 파일 트리 로드
  const loadFiles = useCallback(async () => {
    try {
      const headers = await authHeaders(isPrivateVault);
      const r = await fetch(buildApiUrl("/files"), { headers });
      const data = await r.json();
      setFiles(sortFiles(data.files || []));
    } catch (err) {
      console.error('Failed to load files:', err);
    }
  }, [userId, sortFiles]);

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
      (async () => {
        try {
          const headers = await authHeaders(isPrivateVault);
          const r = await fetch(buildApiUrl(`/file/${encodePath(fp)}`), { headers });
          const data = await r.json();
          if (!data.error) {
            setCurrentFile(data);
            setContent(data.content);
            setSavedContent(data.content);
          }
        } catch {}
      })();
    } else {
      setCurrentFile(null);
      setContent('');
      setSavedContent('');
    }
  }, [filePath, userId]);

  // 컨텍스트 메뉴 닫기 (클릭 또는 터치이동)
  useEffect(() => {
    const handler = () => setContextMenu(null);
    window.addEventListener('click', handler);
    window.addEventListener('touchmove', handler, { passive: true });
    return () => {
      window.removeEventListener('click', handler);
      window.removeEventListener('touchmove', handler);
    };
  }, []);

  // Firebase 변경 감지 (refs 사용 → 리스너 재구독 최소화)
  useEffect(() => {
    if (isPrivateVault) return; // Private Vault에서는 Firebase 리스너 불필요
    const unsubscribe = onFilesChanged(userId, async (changedFiles) => {
      if (currentFile) {
        const changed = changedFiles.find(f => f.path === currentFile.path);
        if (changed) {
          // 자기 자신이 방금 저장한 변경이면 스킵
          if (changed.hash === lastSavedHashRef.current) {
            lastSavedHashRef.current = null;
            loadFiles();
            return;
          }
          // 사용자가 편집 중이면 (미저장 변경이 있으면) 스킵
          if (contentRef.current !== savedContentRef.current) {
            loadFiles();
            return;
          }
          // 원격 변경만 반영
          if (changed.hash !== simpleHash(contentRef.current)) {
            try {
              const headers = await authHeaders(isPrivateVault);
              const r = await fetch(buildApiUrl(`/file/${encodePath(currentFile.path)}`), { headers });
              const data = await r.json();
              setContent(data.content);
              setSavedContent(data.content);
              setSaveStatus('idle');
            } catch (err) {
              console.error('Failed to reload:', err);
            }
          }
        }
      }
      loadFiles();
    });
    return () => unsubscribe && unsubscribe();
  }, [currentFile, loadFiles, userId]);

  // 파일 열기 (URL 변경 + 최근 파일 기록)
  const openFile = useCallback((fp) => {
    navigate(isPrivateVault ? `/private/${fp}` : `/${userId}/${fp}`);
    setRecentFiles(prev => {
      const updated = [fp, ...prev.filter(f => f !== fp)].slice(0, 10);
      localStorage.setItem('mdflare-recent', JSON.stringify(updated));
      return updated;
    });
  }, [userId, isPrivateVault, navigate]);

  // 자동 저장 (savedContentRef 사용 → 불필요한 재생성 방지)
  const doSave = useCallback(async (fp, newContent) => {
    setSaveStatus('saving');
    try {
      const prev = savedContentRef.current;
      const oldHash = simpleHash(prev);
      const newHash = simpleHash(newContent);
      const diff = computeLineDiff(prev, newContent);
      const res = await fetch(buildApiUrl(`/file/${encodePath(fp)}`), {
        method: 'PUT',
        headers: await authHeaders(isPrivateVault),
        body: JSON.stringify({ content: newContent, oldHash, diff })
      });
      const data = await res.json();
      if (data.saved) {
        lastSavedHashRef.current = newHash;
        setSavedContent(newContent);
        setSaveStatus('saved');
        // Worker가 RTDB에 기록하므로 여기서 updateFileMeta 호출 불필요
        setTimeout(() => setSaveStatus(s => s === 'saved' ? 'idle' : s), 2000);
      }
    } catch (err) {
      console.error('Failed to save:', err);
      setSaveStatus('error');
    }
  }, [userId, isPrivateVault]);

  const handleChange = useCallback((val) => {
    setContent(val);
    if (saveTimer.current) clearTimeout(saveTimer.current);
    if (val !== savedContentRef.current && currentFile) {
      setSaveStatus('editing');
      saveTimer.current = setTimeout(() => {
        doSave(currentFile.path, val);
      }, AUTO_SAVE_DELAY);
    }
  }, [currentFile, doSave]);

  useEffect(() => {
    return () => { if (saveTimer.current) clearTimeout(saveTimer.current); };
  }, []);

  // 파일 트리 검색 필터
  const filterFiles = useCallback((items, query) => {
    if (!query) return items;
    const q = query.toLowerCase();
    return items.reduce((acc, item) => {
      if (item.type === 'folder') {
        const filteredChildren = filterFiles(item.children || [], query);
        if (filteredChildren.length > 0 || item.name.toLowerCase().includes(q)) {
          acc.push({ ...item, children: filteredChildren });
        }
      } else if (item.name.toLowerCase().includes(q)) {
        acc.push(item);
      }
      return acc;
    }, []);
  }, []);

  const filteredFiles = searchQuery ? filterFiles(files, searchQuery) : files;

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
      const newContent = `# ${name.replace('.md', '')}\n\n`;
      await fetch(buildApiUrl(`/file/${encodePath(fp)}`), {
        method: 'PUT',
        headers: await authHeaders(isPrivateVault),
        body: JSON.stringify({ content: newContent })
      });
      if (!isPrivateVault) {
        updateFileMeta(userId, fp, {
          size: new Blob([newContent]).size,
          hash: simpleHash(newContent),
          action: 'create'
        }).catch(err => console.error('Firebase meta update failed:', err));
      }
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
        await fetch(buildApiUrl(`/file/${encodePath(s.path)}`), {
          method: 'PUT',
          headers: await authHeaders(isPrivateVault),
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

  // 파일/폴더 이동
  const handleMove = async (sourcePath, targetFolder) => {
    const name = sourcePath.split('/').pop();
    const sourceParent = sourcePath.includes('/') ? sourcePath.substring(0, sourcePath.lastIndexOf('/')) : '';
    // 같은 폴더로 이동 시 무시
    if (sourceParent === targetFolder) return;
    const newPath = targetFolder ? `${targetFolder}/${name}` : name;
    if (sourcePath === newPath) return;
    if (newPath.startsWith(sourcePath + '/')) {
      addToast('❌ 자기 자신의 하위로 이동할 수 없습니다', 'error', 3000);
      return;
    }
    const tid = addToast(`📦 "${name}" 이동 중...`, 'loading');
    setSidebarLoading(true);
    try {
      await fetch(buildApiUrl("/rename"), {
        method: 'POST',
        headers: await authHeaders(isPrivateVault),
        body: JSON.stringify({ oldPath: sourcePath, newPath })
      });
      if (!isPrivateVault) {
        deleteFileMeta(userId, sourcePath).catch(err => console.error('Firebase delete old meta failed:', err));
        updateFileMeta(userId, newPath, {
          size: 0,
          hash: '',
          action: 'rename',
          oldPath: sourcePath
        }).catch(err => console.error('Firebase move meta failed:', err));
      }
      await loadFiles();
      updateToast(tid, `📦 "${name}" 이동 완료!`, 'success');
      if (currentFile?.path === sourcePath) openFile(newPath);
    } catch (err) {
      console.error('Failed to move:', err);
      updateToast(tid, `📦 이동 실패`, 'error');
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
      await fetch(buildApiUrl(`/file/${encodePath(fp)}`), {
        method: 'PUT',
        headers: await authHeaders(isPrivateVault),
        body: JSON.stringify({ content: '' })
      });
      if (!isPrivateVault) {
        updateFileMeta(userId, fp, {
          size: 0,
          hash: simpleHash(''),
          action: 'create'
        }).catch(err => console.error('Firebase meta update failed:', err));
      }
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
      await fetch(buildApiUrl("/rename"), {
        method: 'POST',
        headers: await authHeaders(isPrivateVault),
        body: JSON.stringify({ oldPath, newPath })
      });
      if (!isPrivateVault) {
        // 이전 경로 RTDB 엔트리 삭제 + 새 경로에 rename 기록
        deleteFileMeta(userId, oldPath).catch(err => console.error('Firebase delete old meta failed:', err));
        updateFileMeta(userId, newPath, {
          size: 0,
          hash: '',
          action: 'rename',
          oldPath
        }).catch(err => console.error('Firebase rename meta failed:', err));
      }
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
      await fetch(buildApiUrl(`/file/${encodePath(fp)}${folderQuery}`), { method: 'DELETE', headers: await authHeaders(isPrivateVault) });
      if (!isPrivateVault) {
        deleteFileMeta(userId, fp).catch(err => console.error('Firebase delete meta failed:', err));
      }
      await loadFiles();
      updateToast(tid, `🗑️ "${name}" ${label} 삭제 완료`, 'success');
      if (currentFile?.path === fp || (isFolder && currentFile?.path?.startsWith(fp + '/'))) {
        navigate(isPrivateVault ? '/private' : `/${userId}`);
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
      const headers = await authHeaders(isPrivateVault);
      const res = await fetch(buildApiUrl(`/file/${encodePath(fp)}`), { headers });
      const data = await res.json();
      const ext = fp.lastIndexOf('.md');
      const newPath = ext > 0 ? `${fp.slice(0, ext)} (copy).md` : `${fp} (copy)`;
      await fetch(buildApiUrl(`/file/${encodePath(newPath)}`), {
        method: 'PUT',
        headers: await authHeaders(isPrivateVault),
        body: JSON.stringify({ content: data.content })
      });
      if (!isPrivateVault) {
        updateFileMeta(userId, newPath, {
          size: new Blob([data.content]).size,
          hash: simpleHash(data.content),
          action: 'create'
        }).catch(err => console.error('Firebase meta update failed:', err));
      }
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
    if (isPrivateVault) {
      localStorage.removeItem('mdflare_mode');
      localStorage.removeItem('mdflare_server_url');
      localStorage.removeItem('mdflare_token');
      localStorage.removeItem('mdflare_use_proxy');
    } else {
      await logout();
    }
    navigate('/');
  };

  // 저장 안 된 변경사항 경고 (브라우저 닫기/새로고침)
  useEffect(() => {
    const handler = (e) => {
      if (content !== savedContent) {
        e.preventDefault();
        e.returnValue = '';
      }
    };
    window.addEventListener('beforeunload', handler);
    return () => window.removeEventListener('beforeunload', handler);
  }, [content, savedContent]);

  // 테마 적용
  useEffect(() => {
    document.body.classList.toggle('light-mode', lightMode);
    localStorage.setItem('mdflare-theme', lightMode ? 'light' : 'dark');
  }, [lightMode]);

  // 키보드 단축키
  useEffect(() => {
    const handler = (e) => {
      // Ctrl/Cmd + S: 즉시 저장
      if ((e.ctrlKey || e.metaKey) && e.key === 's') {
        e.preventDefault();
        if (currentFile && content !== savedContent) {
          if (saveTimer.current) clearTimeout(saveTimer.current);
          doSave(currentFile.path, content);
        }
      }
      // Ctrl/Cmd + B: 사이드바 토글
      if ((e.ctrlKey || e.metaKey) && e.key === 'b') {
        e.preventDefault();
        setSidebarOpen(prev => !prev);
      }
      // Escape: 검색 초기화 또는 컨텍스트 메뉴 닫기
      if (e.key === 'Escape') {
        setSearchQuery('');
        setContextMenu(null);
      }
    };
    window.addEventListener('keydown', handler);
    return () => window.removeEventListener('keydown', handler);
  }, [currentFile, content, savedContent, doSave]);

  const statusClass = { idle: 'idle', editing: 'unsaved', saving: 'saving', saved: 'saved', error: 'error' };
  const statusTitle = { idle: '', editing: '수정됨', saving: '저장 중...', saved: '저장됨', error: '저장 실패' };

  return (
    <>
      <header className="header">
        <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
          <button className="sidebar-toggle" onClick={() => setSidebarOpen(!sidebarOpen)}>
            {sidebarOpen ? '✕' : '☰'}
          </button>
          <h1 onClick={() => navigate(isPrivateVault ? '/private' : `/${userId}`)} style={{ cursor: 'pointer' }}>🔥 {getAppName()}</h1>
        </div>
        <div style={{ display: 'flex', alignItems: 'center', gap: 12 }}>
          <AgentStatus userId={userId} isPrivateVault={isPrivateVault} />
          <span className="user-badge">{isPrivateVault ? '🔐 Private Vault' : `👤 ${user?.displayName || userId}`}</span>
          <button className="logout-btn" onClick={handleLogout}>{isPrivateVault ? '연결 해제' : '로그아웃'}</button>
        </div>
      </header>

      <div className="main">
        <aside className={`sidebar ${sidebarOpen ? 'open' : ''}`}>
          {!sidebarOpen && (
            <div className="sidebar-collapsed" onClick={() => setSidebarOpen(true)}>
              ▼
            </div>
          )}
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
          <div className="sidebar-search">
            <input type="text" placeholder="🔍 파일 검색..." value={searchQuery} onChange={(e) => setSearchQuery(e.target.value)} className="search-input" />
            {searchQuery && <button className="search-clear" onClick={() => setSearchQuery('')}>✕</button>}
          </div>
          <div className="file-tree" onContextMenu={(e) => {
            e.preventDefault();
            if (e.target.closest('.tree-item')) return;
            showContextMenu(e, 'root', '', 'root');
          }}
            onDragOver={(e) => { e.preventDefault(); e.dataTransfer.dropEffect = 'move'; }}
            onDrop={(e) => { e.preventDefault(); const src = e.dataTransfer.getData('text/plain'); if (src) handleMove(src, ''); }}>
            <FileTree items={filteredFiles} currentPath={currentFile?.path} onSelect={openFile} onContextMenu={showContextMenu} focusedFolder={focusedFolder} onFocusFolder={setFocusedFolder} onNewFile={handleNewFile} onDragMove={handleMove} dragOver={dragOver} onDragOver={setDragOver} dragSrc={dragSrc} onDragStart={setDragSrc} />
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
                <span className="file-path">
                  {(() => {
                    const parts = currentFile.path.split('/');
                    return parts.map((part, i) => (
                      <span key={i}>
                        {i > 0 && <span className="breadcrumb-sep">/</span>}
                        {i < parts.length - 1 ? (
                          <span className="breadcrumb-link" onClick={() => {
                            const folderPath = parts.slice(0, i + 1).join('/');
                            setFocusedFolder(folderPath);
                            setSidebarOpen(true);
                          }}>{part}</span>
                        ) : (
                          <span className="breadcrumb-current">{part}</span>
                        )}
                      </span>
                    ));
                  })()}
                </span>
                <div style={{ display: 'flex', gap: 8, alignItems: 'center' }}>
                  <div className="tab-bar">
                    <button className={`tab-btn ${view === 'edit' ? 'active' : ''}`} onClick={() => setView('edit')}>Edit</button>
                    <button className={`tab-btn ${view === 'split' ? 'active' : ''}`} onClick={() => setView('split')}>Split</button>
                    <button className={`tab-btn ${view === 'preview' ? 'active' : ''}`} onClick={() => setView('preview')}>Preview</button>
                  </div>
                  <button className="tab-btn" onClick={() => {
                    const cols = parseInt(prompt('열 개수:', '3'));
                    if (!cols || cols < 1) return;
                    const header = '| ' + Array.from({length: cols}, (_, i) => `제목${i+1}`).join(' | ') + ' |';
                    const sep = '| ' + Array.from({length: cols}, () => '---').join(' | ') + ' |';
                    const row = '| ' + Array.from({length: cols}, () => '  ').join(' | ') + ' |';
                    setContent(prev => prev + '\n' + header + '\n' + sep + '\n' + row + '\n');
                    addToast('📊 테이블 삽입됨', 'success', 2000);
                  }} title="테이블 삽입">📊</button>
                  <button className="tab-btn" onClick={() => {
                    const url = prompt('이미지 URL:');
                    if (!url) return;
                    const alt = prompt('대체 텍스트:', 'image') || 'image';
                    const md = `![${alt}](${url})`;
                    setContent(prev => prev + '\n' + md + '\n');
                    addToast('🖼️ 이미지 삽입됨', 'success', 2000);
                  }} title="이미지 삽입">🖼️</button>
                  {!isPrivateVault && (
                  <button className="tab-btn" onClick={async () => {
                    const shareUrl = `${window.location.origin}/${userId}/${currentFile.path}`;
                    try {
                      await navigator.clipboard.writeText(shareUrl);
                      addToast(`🔗 공유 링크 복사됨`, 'success', 2000);
                    } catch {
                      prompt('공유 링크:', shareUrl);
                    }
                  }} title="공유 링크 복사">🔗</button>
                  )}
                  <button className="tab-btn" onClick={() => setLightMode(!lightMode)} title="테마 전환">{lightMode ? '🌙' : '☀️'}</button>
                  <button className="tab-btn" onClick={() => {
                    if (document.fullscreenElement) {
                      document.exitFullscreen();
                    } else {
                      document.documentElement.requestFullscreen();
                    }
                  }} title="전체화면 토글">⛶</button>
                  <button className="tab-btn" onClick={async () => {
                    try {
                      await navigator.clipboard.writeText(content);
                      addToast('📋 클립보드에 복사됨', 'success', 2000);
                    } catch { addToast('📋 복사 실패', 'error', 2000); }
                  }} title="전체 내용 복사">📋</button>
                  <button className="tab-btn" onClick={() => {
                    const blob = new Blob([content], { type: 'text/markdown' });
                    const url = URL.createObjectURL(blob);
                    const a = document.createElement('a');
                    a.href = url;
                    a.download = currentFile.path.split('/').pop();
                    a.click();
                    URL.revokeObjectURL(url);
                    addToast('💾 다운로드 시작', 'success', 2000);
                  }} title="파일 다운로드">💾</button>
                  <span className={`save-status ${statusClass[saveStatus]}`} title={statusTitle[saveStatus]} />
                </div>
              </div>
              <div className="editor-stats">
                <span>{content.length}자</span>
                <span>{content.trim() ? content.trim().split(/\s+/).length : 0}단어</span>
                <span>{content.split('\n').length}줄</span>
              </div>
              <div className="editor-content">
                {(view === 'edit' || view === 'split') && (
                  <CodeMirror value={content} onChange={handleChange}
                    extensions={cmExtensions}
                    theme="none" style={cmStyle} />
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
            <div className="empty-state"
              onDragOver={(e) => { e.preventDefault(); e.dataTransfer.dropEffect = 'copy'; }}
              onDrop={async (e) => {
                e.preventDefault();
                const droppedFiles = [...e.dataTransfer.files].filter(f => f.name.endsWith('.md') || f.name.endsWith('.txt') || f.name.endsWith('.markdown'));
                if (droppedFiles.length === 0) return;
                const tid = addToast(`📤 ${droppedFiles.length}개 파일 업로드 중...`, 'loading');
                setSidebarLoading(true);
                try {
                  for (const file of droppedFiles) {
                    const text = await file.text();
                    const targetFolder = focusedFolder || '';
                    const fp = targetFolder ? `${targetFolder}/${file.name}` : file.name;
                    await fetch(buildApiUrl(`/file/${encodePath(fp)}`), {
                      method: 'PUT',
                      headers: await authHeaders(isPrivateVault),
                      body: JSON.stringify({ content: text })
                    });
                    if (!isPrivateVault) {
                      updateFileMeta(userId, fp, {
                        size: new Blob([text]).size,
                        hash: simpleHash(text),
                        action: 'create'
                      }).catch(err => console.error('Firebase meta update failed:', err));
                    }
                  }
                  await loadFiles();
                  updateToast(tid, `📤 ${droppedFiles.length}개 파일 업로드 완료!`, 'success');
                } catch (err) {
                  updateToast(tid, '📤 업로드 실패', 'error');
                } finally {
                  setSidebarLoading(false);
                }
              }}>
              <div className="logo">🔥</div>
              <p>파일을 선택하거나 .md 파일을 여기에 드롭하세요</p>
              {recentFiles.length > 0 && (
                <div className="recent-files">
                  <h4>최근 파일</h4>
                  {recentFiles.map(fp => (
                    <div key={fp} className="recent-item" onClick={() => openFile(fp)}>
                      📄 {fp}
                    </div>
                  ))}
                </div>
              )}
            </div>
          )}
        </div>
      </div>

      {contextMenu && (
        <ContextMenu {...contextMenu}
          onNewFile={handleNewFile} onNewFolder={handleNewFolder}
          onRename={handleRename} onDelete={handleDelete}
          onDuplicate={handleDuplicate}
          onClose={() => setContextMenu(null)} />
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

function FileTree({ items, currentPath, onSelect, onContextMenu, focusedFolder, onFocusFolder, onNewFile, onDragMove, dragOver, onDragOver, dragSrc, onDragStart, depth = 0 }) {
  return items.map((item) => (
    <div key={item.path}>
      {item.type === 'folder' ? (
        <FolderItem item={item} currentPath={currentPath} onSelect={onSelect} onContextMenu={onContextMenu} focusedFolder={focusedFolder} onFocusFolder={onFocusFolder} onNewFile={onNewFile} onDragMove={onDragMove} dragOver={dragOver} onDragOver={onDragOver} dragSrc={dragSrc} onDragStart={onDragStart} depth={depth} />
      ) : (
        <FileItem item={item} currentPath={currentPath} onSelect={onSelect} onContextMenu={onContextMenu} onDragMove={onDragMove} onDragStart={onDragStart} depth={depth} />
      )}
    </div>
  ));
}

function FileItem({ item, currentPath, onSelect, onContextMenu, onDragMove, onDragStart, depth }) {
  const longPressHandlers = useLongPress(
    (e) => onContextMenu(e, 'file', item.path, item.name),
    () => onSelect(item.path),
  );
  return (
    <div className={`tree-item ${item.path === currentPath ? 'active' : ''}`}
      style={{ paddingLeft: 16 + depth * 16 }}
      draggable
      onDragStart={(e) => { e.dataTransfer.setData('text/plain', item.path); e.dataTransfer.effectAllowed = 'move'; onDragStart(item.path); }}
      onDragEnd={() => onDragStart(null)}
      {...longPressHandlers}>
      <span className="icon">📄</span>
      <span className="tree-item-name">{item.name}</span>
      <button className="tree-item-menu" onClick={(e) => { e.stopPropagation(); onContextMenu(e, 'file', item.path, item.name); }}>⋮</button>
    </div>
  );
}

function FolderItem({ item, currentPath, onSelect, onContextMenu, focusedFolder, onFocusFolder, onNewFile, onDragMove, dragOver, onDragOver, dragSrc, onDragStart, depth }) {
  const [open, setOpen] = useState(true);
  const isFocused = focusedFolder === item.path;
  const srcParent = dragSrc?.includes('/') ? dragSrc.substring(0, dragSrc.lastIndexOf('/')) : '';
  const isSameFolder = dragSrc && srcParent === item.path;
  const isDragOver = dragOver === item.path && !isSameFolder;
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
      <div className={`tree-item tree-folder ${isFocused ? 'focused' : ''} ${isDragOver ? 'drag-over' : ''}`}
        style={{ paddingLeft: 16 + depth * 16 }}
        onDragOver={(e) => {
          e.preventDefault();
          if (isSameFolder) { e.dataTransfer.dropEffect = 'none'; return; }
          e.dataTransfer.dropEffect = 'move';
          onDragOver(item.path);
        }}
        onDragLeave={() => onDragOver(null)}
        onDrop={(e) => { e.preventDefault(); e.stopPropagation(); const src = e.dataTransfer.getData('text/plain'); onDragOver(null); if (src && !isSameFolder && onDragMove) onDragMove(src, item.path); }}
        {...longPressHandlers}>
        <span className="icon">{open ? '📂' : '📁'}</span>
        <span className="tree-item-name">{item.name}</span>
        <button className="tree-item-menu" onClick={(e) => { e.stopPropagation(); onContextMenu(e, 'folder', item.path, item.name); }}>⋮</button>
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
            <FileTree items={visibleChildren} currentPath={currentPath} onSelect={onSelect} onContextMenu={onContextMenu} focusedFolder={focusedFolder} onFocusFolder={onFocusFolder} onNewFile={onNewFile} onDragMove={onDragMove} dragOver={dragOver} onDragOver={onDragOver} dragSrc={dragSrc} onDragStart={onDragStart} depth={depth + 1} />
          )}
        </div>
      )}
    </>
  );
}
