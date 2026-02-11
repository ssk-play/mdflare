import React, { useState, useCallback, useRef, useEffect } from 'react';
import { useNavigate } from 'react-router-dom';
import { getAppName } from '@mdflare/common';

const USERNAME_REGEX = /^[a-z0-9][a-z0-9-]{1,18}[a-z0-9]$/;

export default function SetUsername({ user }) {
  const navigate = useNavigate();
  const [username, setUsername] = useState('');
  const [status, setStatus] = useState('idle'); // idle | checking | available | taken | invalid | error
  const [message, setMessage] = useState('');
  const [submitting, setSubmitting] = useState(false);
  const checkTimer = useRef(null);

  // 입력할 때마다 debounce 체크
  const handleChange = useCallback((e) => {
    const val = e.target.value.toLowerCase().replace(/[^a-z0-9-]/g, '');
    setUsername(val);

    if (checkTimer.current) clearTimeout(checkTimer.current);

    if (!val || val.length < 3) {
      setStatus('idle');
      setMessage(val.length > 0 ? '3자 이상 입력해주세요' : '');
      return;
    }

    if (!USERNAME_REGEX.test(val)) {
      setStatus('invalid');
      setMessage('영문소문자, 숫자, 하이픈만 가능 (시작/끝은 영문 또는 숫자)');
      return;
    }

    setStatus('checking');
    setMessage('확인 중...');

    checkTimer.current = setTimeout(async () => {
      try {
        const res = await fetch(`/api/username/check?name=${val}`);
        const data = await res.json();
        if (data.available) {
          setStatus('available');
          setMessage(`✓ ${val} 사용 가능!`);
        } else {
          setStatus('taken');
          setMessage(data.reason || '이미 사용 중인 이름입니다');
        }
      } catch {
        setStatus('error');
        setMessage('확인 실패. 다시 시도해주세요.');
      }
    }, 400);
  }, []);

  const handleSubmit = async (e) => {
    e.preventDefault();
    if (status !== 'available' || submitting) return;

    setSubmitting(true);
    try {
      const res = await fetch('/api/username/register', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          uid: user.uid,
          username,
          displayName: user.displayName || ''
        })
      });
      const data = await res.json();
      if (data.registered) {
        const returnTo = new URLSearchParams(window.location.search).get('return');
        navigate(returnTo === 'agent' ? '/auth/agent' : `/${username}`);
      } else {
        setStatus('error');
        setMessage(data.error || '등록 실패');
      }
    } catch {
      setStatus('error');
      setMessage('등록 실패. 다시 시도해주세요.');
    } finally {
      setSubmitting(false);
    }
  };

  useEffect(() => {
    return () => { if (checkTimer.current) clearTimeout(checkTimer.current); };
  }, []);

  const statusColor = {
    idle: '#888', checking: '#888', available: '#3fb950',
    taken: '#f85149', invalid: '#f85149', error: '#f85149'
  };

  return (
    <div className="landing">
      <nav className="landing-nav">
        <h1>🔥 {getAppName()}</h1>
      </nav>

      <div className="hero" style={{ maxWidth: 480 }}>
        <h2>사용자 이름 설정</h2>
        <p className="hero-sub">
          URL에 사용될 이름을 정해주세요.<br />
          <strong>mdflare.com/<span style={{ color: '#58a6ff' }}>{username || 'your-name'}</span></strong>
        </p>

        <form onSubmit={handleSubmit} style={{ marginTop: 24 }}>
          <div style={{ position: 'relative' }}>
            <input
              type="text"
              value={username}
              onChange={handleChange}
              placeholder="your-username"
              maxLength={20}
              autoFocus
              style={{
                width: '100%',
                padding: '12px 16px',
                fontSize: 18,
                background: '#161b22',
                border: `1px solid ${status === 'available' ? '#3fb950' : status === 'taken' || status === 'invalid' ? '#f85149' : '#30363d'}`,
                borderRadius: 8,
                color: '#e6edf3',
                outline: 'none',
                boxSizing: 'border-box',
                transition: 'border-color 0.2s'
              }}
            />
          </div>
          {message && (
            <p style={{ color: statusColor[status], marginTop: 8, fontSize: 14, textAlign: 'left' }}>
              {message}
            </p>
          )}
          <button
            type="submit"
            className="cta-btn"
            disabled={status !== 'available' || submitting}
            style={{
              marginTop: 20,
              width: '100%',
              opacity: status === 'available' && !submitting ? 1 : 0.5,
              cursor: status === 'available' && !submitting ? 'pointer' : 'not-allowed'
            }}
          >
            {submitting ? '등록 중...' : '이 이름으로 시작하기 →'}
          </button>
        </form>
      </div>
    </div>
  );
}
