import { useEffect, useState } from 'react';
import { auth, googleProvider } from '../firebase';
import { signInWithPopup, onAuthStateChanged } from 'firebase/auth';

export default function AgentAuth() {
  const [status, setStatus] = useState('loading');
  const [error, setError] = useState(null);
  const [user, setUser] = useState(null);
  const [username, setUsername] = useState(null);

  useEffect(() => {
    const unsub = onAuthStateChanged(auth, (u) => {
      if (u) {
        setUser(u);
        setStatus('logged_in');
      } else {
        setStatus('ready');
      }
    });
    return () => unsub();
  }, []);

  const handleLogin = async () => {
    setStatus('logging_in');
    try {
      const result = await signInWithPopup(auth, googleProvider);
      setUser(result.user);
      setStatus('logged_in');
    } catch (err) {
      setError(err.message);
      setStatus('error');
    }
  };

  const handleAuthorize = async () => {
    if (!user) return;
    setStatus('authorizing');
    
    try {
      // username 조회
      const res = await fetch(`/api/username/resolve?uid=${user.uid}`);
      const data = await res.json();
      
      if (!data.username) {
        setError('먼저 웹에서 username을 설정해주세요.');
        setStatus('error');
        return;
      }

      // 새 토큰 생성 (기존 토큰 유지하면서 추가 토큰 발급)
      const idToken = await user.getIdToken();
      const tokenRes = await fetch('/api/token/agent', {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
          'Authorization': `Bearer ${idToken}`
        },
        body: JSON.stringify({ uid: user.uid, username: data.username })
      });
      const tokenData = await tokenRes.json();

      if (!tokenData.token) {
        setError(tokenData.error || '토큰 생성 실패');
        setStatus('error');
        return;
      }

      setUsername(data.username);

      // mdflare:// URL scheme으로 리다이렉트
      const callbackUrl = `mdflare://callback?uid=${encodeURIComponent(user.uid)}&username=${encodeURIComponent(data.username)}&token=${encodeURIComponent(tokenData.token)}`;
      
      setStatus('redirecting');

      // 앱이 열리면 브라우저가 포커스를 잃음
      let appOpened = false;
      const onBlur = () => {
        appOpened = true;
        setStatus('done');
        window.removeEventListener('blur', onBlur);
      };
      window.addEventListener('blur', onBlur);

      window.location.href = callbackUrl;

      // 3초 후 앱이 안 열렸으면 설치 안내
      setTimeout(() => {
        window.removeEventListener('blur', onBlur);
        if (!appOpened) {
          setStatus('app_not_found');
        }
      }, 3000);

    } catch (err) {
      setError(err.message);
      setStatus('error');
    }
  };

  return (
    <div className="agent-auth">
      <div className="agent-auth-card">
        <div className="logo">🔥</div>
        <h1>MDFlare 에이전트 로그인</h1>
        
        {status === 'loading' && (
          <p className="status">로딩 중...</p>
        )}

        {status === 'ready' && (
          <>
            <p>Google 계정으로 로그인하여 에이전트를 연결하세요.</p>
            <button className="auth-btn" onClick={handleLogin}>
              🔐 Google 로그인
            </button>
          </>
        )}

        {status === 'logging_in' && (
          <p className="status">로그인 중...</p>
        )}

        {status === 'logged_in' && user && (
          <>
            <p className="user-info">
              👤 {user.displayName || user.email}
            </p>
            <button className="auth-btn primary" onClick={handleAuthorize}>
              ✅ 에이전트 연결 승인
            </button>
            <p className="hint">버튼을 누르면 MDFlare 에이전트 앱이 열립니다.</p>
          </>
        )}

        {status === 'authorizing' && (
          <p className="status">승인 처리 중...</p>
        )}

        {status === 'redirecting' && (
          <p className="status success">✅ 에이전트로 이동 중...</p>
        )}

        {status === 'done' && (
          <>
            <p className="status success">✅ 에이전트에 연결되었습니다.</p>
            <a href={`/${username}`} className="auth-btn primary" style={{display:'inline-block',textDecoration:'none',marginTop:'12px'}}>
              📝 내 페이지로 이동
            </a>
          </>
        )}

        {status === 'app_not_found' && (
          <div className="error-box">
            <p>⚠️ MDFlare 에이전트 앱을 찾을 수 없습니다.</p>
            <a href="/download" className="download-link">에이전트 다운로드 →</a>
          </div>
        )}

        {status === 'error' && (
          <div className="error-box">
            <p>❌ {error}</p>
            <button className="retry-btn" onClick={() => setStatus('ready')}>다시 시도</button>
          </div>
        )}
      </div>
    </div>
  );
}
