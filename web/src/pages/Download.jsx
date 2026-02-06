import React, { useState, useEffect } from 'react';
import { useNavigate } from 'react-router-dom';
import { getAppName } from '../components/AppTitle';

const STORAGE_BASE = 'https://firebasestorage.googleapis.com/v0/b/markdownflare.firebasestorage.app/o';
const META_URL = `${STORAGE_BASE}/downloads%2Fmac%2Fmeta.json?alt=media`;
const downloadUrl = (meta) =>
  `${STORAGE_BASE}/downloads%2Fmac%2FMDFlare-Agent-${meta.version}%2B${meta.build}-mac.zip?alt=media`;

export default function Download() {
  const navigate = useNavigate();
  const [macMeta, setMacMeta] = useState(null);

  useEffect(() => {
    fetch(META_URL).then(r => r.json()).then(setMacMeta).catch(() => {});
  }, []);

  return (
    <div className="landing">
      <nav className="landing-nav">
        <h1 onClick={() => navigate('/')} style={{ cursor: 'pointer' }}>🔥 {getAppName()}</h1>
        <div style={{ display: 'flex', gap: 12 }}>
          <button className="login-btn" onClick={() => navigate('/')}>홈</button>
        </div>
      </nav>

      <div className="hero">
        <h2>로컬 동기화 에이전트</h2>
        <p className="hero-sub">
          내 컴퓨터의 마크다운 폴더를 MDFlare와 실시간 동기화.<br />
          파일을 로컬에서 수정하면 웹에 즉시 반영됩니다.
        </p>
      </div>

      <div className="download-cards">
        <div className="download-card">
          <span className="download-icon">🍎</span>
          <h3>macOS</h3>
          <p>Apple Silicon & Intel 지원</p>
          <a
            href={macMeta ? downloadUrl(macMeta) : '#'}
            className="cta-btn"
            style={{ textDecoration: 'none', display: 'inline-block' }}
          >
            다운로드{macMeta ? ` (${macMeta.size})` : ''}
          </a>
          <span className="download-note">Rust 네이티브 · 시스템 트레이{macMeta ? ` · v${macMeta.version}+${macMeta.build}` : ''}</span>
          <span className="download-note" style={{ marginTop: 4 }}>zip 해제 후 실행</span>
        </div>

        <div className="download-card">
          <span className="download-icon">🪟</span>
          <h3>Windows</h3>
          <p>Windows 10 이상</p>
          <button className="cta-btn" disabled style={{ opacity: 0.5, cursor: 'not-allowed' }}>
            Coming Soon
          </button>
          <span className="download-note">시스템 트레이 · 백그라운드 동기화</span>
        </div>

        <div className="download-card">
          <span className="download-icon">🐧</span>
          <h3>Linux</h3>
          <p>Ubuntu, Fedora, Arch 등</p>
          <button className="cta-btn" disabled style={{ opacity: 0.5, cursor: 'not-allowed' }}>
            Coming Soon
          </button>
          <span className="download-note">CLI + 데몬 · AppImage</span>
        </div>
      </div>

      <div className="how-it-works">
        <h2>어떻게 동작하나요?</h2>
        <div className="steps">
          <div className="step">
            <span className="step-num">1</span>
            <h3>에이전트 설치</h3>
            <p>OS에 맞는 에이전트를 다운로드하고 설치합니다.</p>
          </div>
          <div className="step">
            <span className="step-num">2</span>
            <h3>폴더 연결</h3>
            <p>동기화할 로컬 마크다운 폴더를 선택합니다.</p>
          </div>
          <div className="step">
            <span className="step-num">3</span>
            <h3>자동 동기화</h3>
            <p>파일 변경을 감지해 자동으로 클라우드와 동기화합니다.</p>
          </div>
        </div>
      </div>

      <footer className="landing-footer">
        <p>© 2026 MDFlare · Built with Cloudflare</p>
        <p style={{ marginTop: 4, fontSize: 11, color: '#30363d' }}>v{__BUILD_VERSION__} · {__BUILD_TIME__}</p>
      </footer>
    </div>
  );
}
