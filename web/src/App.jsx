import React, { useState, useEffect } from 'react';
import { Routes, Route } from 'react-router-dom';
import { onAuthChange } from './firebase';
import Landing from './pages/Landing';
import Workspace from './pages/Workspace';

export default function App() {
  const [user, setUser] = useState(null);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    const unsub = onAuthChange((u) => {
      setUser(u);
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
      <Route path="/" element={<Landing user={user} />} />
      <Route path="/:userId/*" element={<Workspace user={user} />} />
    </Routes>
  );
}
