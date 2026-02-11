import React, { useEffect } from 'react';
import { useNavigate } from 'react-router-dom';
import { loginWithGoogle, getAppName } from '@mdflare/common';

export default function Landing({ user, username }) {
  const navigate = useNavigate();

  const handleLogin = async () => {
    try {
      const result = await loginWithGoogle();
      const res = await fetch(`/api/username/resolve?uid=${result.user.uid}`);
      const data = await res.json();
      if (data.found) {
        navigate(`/${data.username}`);
      } else {
        navigate('/setup');
      }
    } catch (err) {
      console.error('Login failed:', err);
    }
  };

  useEffect(() => {
    if (user && username) {
      navigate(`/${username}`);
    }
  }, [user, username, navigate]);

  if (user && username) {
    return null;
  }

  return (
    <div className="landing">
      <nav className="landing-nav">
        <h1>☁️ {getAppName()} Cloud</h1>
        <div style={{ display: 'flex', gap: 12, alignItems: 'center' }}>
          <a href="https://home.mdflare.com" className="nav-link">홈</a>
          <button className="nav-link" onClick={() => navigate('/download')}>다운로드</button>
          <button className="login-btn" onClick={handleLogin}>로그인</button>
        </div>
      </nav>

      <div className="hero">
        <h2>Cloud 모드</h2>
        <p className="hero-sub">
          Cloudflare에 저장, 어디서든 접속.<br/>
          Google 로그인으로 간편하게 시작하세요.
        </p>
        
        <div className="mode-buttons">
          <button className="cta-btn" onClick={handleLogin}>
            ☁️ Google로 로그인
          </button>
        </div>
      </div>

      <div className="features">
        <div className="feature-card">
          <span className="feature-icon">☁️</span>
          <h3>클라우드 저장</h3>
          <p>Cloudflare R2에 안전하게 저장</p>
        </div>
        <div className="feature-card">
          <span className="feature-icon">🔄</span>
          <h3>실시간 동기화</h3>
          <p>로컬 에이전트와 양방향 동기화</p>
        </div>
        <div className="feature-card">
          <span className="feature-icon">📱</span>
          <h3>어디서든</h3>
          <p>PC, 태블릿, 스마트폰</p>
        </div>
      </div>

      <footer className="landing-footer">
        <p>© 2026 MDFlare · Built with Cloudflare</p>
        <p style={{ marginTop: 4, fontSize: 11, color: '#30363d' }}>v{__BUILD_VERSION__} · {__BUILD_TIME__}</p>
      </footer>
    </div>
  );
}
