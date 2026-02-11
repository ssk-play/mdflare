import React from 'react';

const CLOUD_URL = 'https://cloud.mdflare.com';
const PRIVATE_URL = 'https://private.mdflare.com';

export default function App() {
  return (
    <div className="landing">
      <nav className="landing-nav">
        <h1>🔥 MDFlare</h1>
        <div style={{ display: 'flex', gap: 12, alignItems: 'center' }}>
          <a href="https://github.com/ssk-play/mdflare" target="_blank" rel="noopener" className="nav-link">GitHub</a>
        </div>
      </nav>

      <div className="hero">
        <h2>내 마크다운 폴더를<br/>웹에서 열다.</h2>
        <p className="hero-sub">
          로컬 마크다운 폴더가 곧 데이터베이스.<br/>
          별도 서버 없이, 어디서든 브라우저로 편집.
        </p>
        
        <div className="mode-buttons">
          <a href={CLOUD_URL} className="cta-btn">
            ☁️ Cloud로 시작하기
          </a>
          <a href={PRIVATE_URL} className="cta-btn secondary">
            🔐 Private Vault
          </a>
        </div>
      </div>

      <div className="features">
        <div className="feature-card">
          <span className="feature-icon">☁️</span>
          <h3>Cloud</h3>
          <p>Cloudflare에 저장, 어디서든 접속.<br/>Google 로그인으로 간편하게.</p>
        </div>
        <div className="feature-card">
          <span className="feature-icon">🔐</span>
          <h3>Private Vault</h3>
          <p>내 PC에만 저장, 완전한 프라이버시.<br/>로컬 에이전트로 터널링.</p>
        </div>
        <div className="feature-card">
          <span className="feature-icon">🔄</span>
          <h3>실시간 동기화</h3>
          <p>로컬에서 수정하면 즉시 반영</p>
        </div>
        <div className="feature-card">
          <span className="feature-icon">✏️</span>
          <h3>웹 에디터</h3>
          <p>브라우저에서 바로 편집</p>
        </div>
        <div className="feature-card">
          <span className="feature-icon">📱</span>
          <h3>모바일 지원</h3>
          <p>스마트폰에서도 편집 가능</p>
        </div>
        <div className="feature-card">
          <span className="feature-icon">🆓</span>
          <h3>오픈소스</h3>
          <p>AGPL-3.0 라이선스</p>
        </div>
      </div>

      <section className="why-section">
        <h2>왜 MDFlare인가?</h2>
        <div className="why-content">
          <h3>Obsidian의 한계</h3>
          <p>
            Obsidian은 훌륭한 마크다운 에디터다. 하지만 치명적인 단점이 있다 — <strong>온라인이 안 된다.</strong>
          </p>
          <ul>
            <li>Obsidian Sync: $8/월, 웹 에디터 없음</li>
            <li>Obsidian Publish: $16/월, 읽기 전용</li>
            <li>Notion: 독자 포맷, AI 연동 불편</li>
          </ul>
          <h3>MDFlare의 해답</h3>
          <p>
            <strong>"로컬 마크다운의 장점 + 웹 접근성"</strong><br/>
            Obsidian vault를 MDFlare에 연결하면 어디서든 브라우저로 편집.
          </p>
        </div>
      </section>

      <footer className="landing-footer">
        <p>© 2026 MDFlare · Built with Cloudflare</p>
        <p style={{ marginTop: 4, fontSize: 11, color: '#30363d' }}>v{__BUILD_VERSION__} · {__BUILD_TIME__}</p>
      </footer>
    </div>
  );
}
