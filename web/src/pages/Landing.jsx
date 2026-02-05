import React from 'react';
import { useNavigate } from 'react-router-dom';
import { loginWithGoogle } from '../firebase';

export default function Landing({ user }) {
  const navigate = useNavigate();

  const handleLogin = async () => {
    try {
      const result = await loginWithGoogle();
      const uid = result.user.uid.substring(0, 8);
      navigate(`/${uid}`);
    } catch (err) {
      console.error('Login failed:', err);
    }
  };

  if (user) {
    navigate(`/${user.uid.substring(0, 8)}`);
    return null;
  }

  return (
    <div className="landing">
      <nav className="landing-nav">
        <h1>🔥 MDFlare</h1>
        <button className="login-btn" onClick={handleLogin}>로그인</button>
      </nav>

      <div className="hero">
        <h2>내 마크다운 폴더를<br/>웹에서 열다.</h2>
        <p className="hero-sub">
          로컬 마크다운 폴더가 곧 데이터베이스.<br/>
          별도 서버 없이, 어디서든 브라우저로 편집.
        </p>
        <button className="cta-btn" onClick={handleLogin}>
          Google로 시작하기 →
        </button>
      </div>

      <div className="features">
        <div className="feature-card">
          <span className="feature-icon">📁</span>
          <h3>로컬 폴더 = DB</h3>
          <p>별도 데이터베이스 없이 내 파일이 곧 데이터</p>
        </div>
        <div className="feature-card">
          <span className="feature-icon">🔄</span>
          <h3>실시간 동기화</h3>
          <p>로컬에서 수정하면 웹에 즉시 반영</p>
        </div>
        <div className="feature-card">
          <span className="feature-icon">✏️</span>
          <h3>웹 마크다운 에디터</h3>
          <p>브라우저에서 바로 편집, 자동 저장</p>
        </div>
        <div className="feature-card">
          <span className="feature-icon">🔒</span>
          <h3>내 파일은 내 소유</h3>
          <p>원본은 항상 내 컴퓨터에</p>
        </div>
      </div>

      <footer className="landing-footer">
        <p>© 2026 MDFlare · Built with Cloudflare</p>
      </footer>
    </div>
  );
}
