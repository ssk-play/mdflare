import React, { useState, useEffect } from 'react';
import { Routes, Route } from 'react-router-dom';
import { onAuthChange } from './firebase';
import Landing from './pages/Landing';
import Workspace from './pages/Workspace';
import SetUsername from './pages/SetUsername';
import Download from './pages/Download';

export default function App() {
  const [user, setUser] = useState(null);
  const [username, setUsername] = useState(null); // resolved username
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    const unsub = onAuthChange(async (u) => {
      setUser(u);
      if (u) {
        // uid → username 조회
        try {
          const res = await fetch(`/api/username/resolve?uid=${u.uid}`);
          const data = await res.json();
          if (data.found) {
            setUsername(data.username);
          } else {
            setUsername(null); // 아직 미설정
          }
        } catch {
          setUsername(null);
        }
      } else {
        setUsername(null);
      }
      setLoading(false);
    });
    return unsub;
  }, []);

  if (loading) {
    return (
      <div className="loading-screen">
        <div className="logo">🔥</div>
        <p>MDFlare</p>
      </div>
    );
  }

  return (
    <Routes>
      <Route path="/" element={<Landing user={user} username={username} />} />
      <Route path="/download" element={<Download />} />
      <Route path="/setup" element={
        user && !username ? <SetUsername user={user} /> : <Landing user={user} username={username} />
      } />
      <Route path="/:userId/*" element={<Workspace user={user} />} />
    </Routes>
  );
}
