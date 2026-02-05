const express = require('express');
const cors = require('cors');
const fs = require('fs');
const path = require('path');

const app = express();
const PORT = 3001;

// 데이터 디렉토리
const DATA_DIR = path.join(__dirname, '..', 'local-data');
const VAULTS_DIR = path.join(DATA_DIR, 'vaults');
const USERS_DIR = path.join(DATA_DIR, 'users');
const USERNAMES_DIR = path.join(DATA_DIR, 'usernames');
const TOKENS_DIR = path.join(DATA_DIR, 'tokens');

// 디렉토리 자동 생성
[DATA_DIR, VAULTS_DIR, USERS_DIR, USERNAMES_DIR, TOKENS_DIR].forEach(dir => {
  if (!fs.existsSync(dir)) fs.mkdirSync(dir, { recursive: true });
});

app.use(cors());
app.use(express.json());

// ──────── Username API ────────

// GET /api/username/check?name=xxx
app.get('/api/username/check', (req, res) => {
  const name = (req.query.name || '').toLowerCase().trim();
  if (!name) return res.json({ error: 'name required' });

  if (!/^[a-z0-9][a-z0-9-]{1,18}[a-z0-9]$/.test(name)) {
    return res.json({ available: false, reason: '3-20자, 영문소문자/숫자/하이픈만 가능' });
  }

  const reserved = ['admin', 'api', 'www', 'app', 'help', 'support', 'login', 'signup', 'settings', 'profile', 'username', 'download', 'setup'];
  if (reserved.includes(name)) {
    return res.json({ available: false, reason: '예약된 이름입니다' });
  }

  const exists = fs.existsSync(path.join(USERNAMES_DIR, `${name}.json`));
  res.json({ available: !exists, username: name });
});

// POST /api/username/register
app.post('/api/username/register', (req, res) => {
  const { uid, username, displayName } = req.body;
  if (!uid || !username) return res.status(400).json({ error: 'uid and username required' });

  const name = username.toLowerCase().trim();
  if (!/^[a-z0-9][a-z0-9-]{1,18}[a-z0-9]$/.test(name)) {
    return res.status(400).json({ error: 'invalid username format' });
  }

  const usernameFile = path.join(USERNAMES_DIR, `${name}.json`);
  if (fs.existsSync(usernameFile)) {
    return res.status(409).json({ error: 'username already taken' });
  }

  const userFile = path.join(USERS_DIR, `${uid}.json`);
  if (fs.existsSync(userFile)) {
    return res.status(409).json({ error: 'user already has a username' });
  }

  const now = new Date().toISOString();
  fs.writeFileSync(usernameFile, JSON.stringify({ uid, createdAt: now }));
  fs.writeFileSync(userFile, JSON.stringify({ username: name, displayName: displayName || '', createdAt: now }));

  // vault 폴더 생성
  const vaultDir = path.join(VAULTS_DIR, uid);
  if (!fs.existsSync(vaultDir)) fs.mkdirSync(vaultDir, { recursive: true });

  res.json({ registered: true, username: name });
});

// GET /api/username/resolve?uid=xxx 또는 ?name=xxx
app.get('/api/username/resolve', (req, res) => {
  const { uid, name } = req.query;

  if (uid) {
    const userFile = path.join(USERS_DIR, `${uid}.json`);
    if (!fs.existsSync(userFile)) return res.json({ found: false });
    const profile = JSON.parse(fs.readFileSync(userFile, 'utf-8'));
    return res.json({ found: true, ...profile });
  }

  if (name) {
    const usernameFile = path.join(USERNAMES_DIR, `${name.toLowerCase()}.json`);
    if (!fs.existsSync(usernameFile)) return res.json({ found: false });
    const data = JSON.parse(fs.readFileSync(usernameFile, 'utf-8'));
    return res.json({ found: true, ...data });
  }

  res.status(400).json({ error: 'uid or name required' });
});

// ──────── Token API ────────

// POST /api/token/generate
app.post('/api/token/generate', (req, res) => {
  const { uid, username } = req.body;
  if (!uid || !username) return res.status(400).json({ error: 'uid and username required' });

  // 기존 토큰 삭제
  const userFile = path.join(USERS_DIR, `${uid}.json`);
  if (fs.existsSync(userFile)) {
    const profile = JSON.parse(fs.readFileSync(userFile, 'utf-8'));
    if (profile.apiToken) {
      const oldTokenFile = path.join(TOKENS_DIR, `${profile.apiToken}.json`);
      if (fs.existsSync(oldTokenFile)) fs.unlinkSync(oldTokenFile);
    }
  }

  // 새 토큰
  const crypto = require('crypto');
  const token = crypto.randomBytes(24).toString('hex');

  fs.writeFileSync(path.join(TOKENS_DIR, `${token}.json`), JSON.stringify({
    uid, username, createdAt: new Date().toISOString()
  }));

  // 프로필 업데이트
  let profile = {};
  if (fs.existsSync(userFile)) {
    profile = JSON.parse(fs.readFileSync(userFile, 'utf-8'));
  }
  profile.apiToken = token;
  fs.writeFileSync(userFile, JSON.stringify(profile));

  res.json({ token });
});

// ──────── 인증 미들웨어 ────────

function resolveUsername(username) {
  const usernameFile = path.join(USERNAMES_DIR, `${username}.json`);
  if (!fs.existsSync(usernameFile)) return { uid: username, ownerUid: null };
  const data = JSON.parse(fs.readFileSync(usernameFile, 'utf-8'));
  return { uid: data.uid, ownerUid: data.uid };
}

function authMiddleware(req, res, next) {
  const username = req.params.userId;
  const { uid, ownerUid } = resolveUsername(username);
  req.resolvedUid = uid;

  // 쓰기 요청은 인증 필요
  if (['PUT', 'POST', 'DELETE'].includes(req.method) && ownerUid) {
    const authHeader = req.headers['authorization'] || '';
    const token = authHeader.replace('Bearer ', '');
    const firebaseUid = req.headers['x-firebase-uid'] || '';

    // Firebase UID
    if (firebaseUid && firebaseUid === ownerUid) return next();

    // API 토큰
    if (token) {
      const tokenFile = path.join(TOKENS_DIR, `${token}.json`);
      if (fs.existsSync(tokenFile)) {
        const tokenData = JSON.parse(fs.readFileSync(tokenFile, 'utf-8'));
        if (tokenData.uid === ownerUid) return next();
      }
    }

    return res.status(403).json({ error: 'Unauthorized' });
  }

  next();
}

// ──────── File API ────────

function getVaultPath(resolvedUid) {
  const vaultPath = path.join(VAULTS_DIR, resolvedUid);
  if (!fs.existsSync(vaultPath)) fs.mkdirSync(vaultPath, { recursive: true });
  return vaultPath;
}

// 파일 트리
app.get('/api/:userId/files', authMiddleware, (req, res) => {
  const vaultPath = getVaultPath(req.resolvedUid);
  const tree = buildFileTree(vaultPath, vaultPath);
  res.json({ user: req.params.userId, files: tree });
});

// 파일 읽기
app.get('/api/:userId/file/*', authMiddleware, (req, res) => {
  const vaultPath = getVaultPath(req.resolvedUid);
  const filePath = path.join(vaultPath, req.params[0]);

  if (!filePath.startsWith(vaultPath)) return res.status(403).json({ error: 'Access denied' });
  if (!fs.existsSync(filePath)) return res.status(404).json({ error: 'File not found' });

  const content = fs.readFileSync(filePath, 'utf-8');
  const stat = fs.statSync(filePath);
  res.json({ path: req.params[0], content, size: stat.size, modified: stat.mtime.toISOString() });
});

// 파일 저장
app.put('/api/:userId/file/*', authMiddleware, (req, res) => {
  const vaultPath = getVaultPath(req.resolvedUid);
  const filePath = path.join(vaultPath, req.params[0]);

  if (!filePath.startsWith(vaultPath)) return res.status(403).json({ error: 'Access denied' });

  const dir = path.dirname(filePath);
  if (!fs.existsSync(dir)) fs.mkdirSync(dir, { recursive: true });

  fs.writeFileSync(filePath, req.body.content, 'utf-8');
  const stat = fs.statSync(filePath);
  res.json({ path: req.params[0], size: stat.size, modified: stat.mtime.toISOString(), saved: true });
});

// 이름 변경
app.post('/api/:userId/rename', authMiddleware, (req, res) => {
  const vaultPath = getVaultPath(req.resolvedUid);
  const { oldPath, newPath } = req.body;
  const oldFull = path.join(vaultPath, oldPath);
  const newFull = path.join(vaultPath, newPath);

  if (!oldFull.startsWith(vaultPath) || !newFull.startsWith(vaultPath)) {
    return res.status(403).json({ error: 'Access denied' });
  }
  if (!fs.existsSync(oldFull)) return res.status(404).json({ error: 'Not found' });

  const newDir = path.dirname(newFull);
  if (!fs.existsSync(newDir)) fs.mkdirSync(newDir, { recursive: true });

  fs.renameSync(oldFull, newFull);
  res.json({ renamed: true, oldPath, newPath });
});

// 파일 삭제
app.delete('/api/:userId/file/*', authMiddleware, (req, res) => {
  const vaultPath = getVaultPath(req.resolvedUid);
  const filePath = path.join(vaultPath, req.params[0]);

  if (!filePath.startsWith(vaultPath)) return res.status(403).json({ error: 'Access denied' });
  if (!fs.existsSync(filePath)) return res.status(404).json({ error: 'File not found' });

  fs.unlinkSync(filePath);
  res.json({ deleted: true, path: req.params[0] });
});

// ──────── 유틸 ────────

function buildFileTree(dirPath, rootPath) {
  const items = [];
  if (!fs.existsSync(dirPath)) return items;
  const entries = fs.readdirSync(dirPath, { withFileTypes: true });

  for (const entry of entries) {
    if (entry.name.startsWith('.')) continue;

    const fullPath = path.join(dirPath, entry.name);
    const relativePath = path.relative(rootPath, fullPath);

    if (entry.isDirectory()) {
      const children = buildFileTree(fullPath, rootPath);
      // 빈 폴더도 표시 (.gitkeep 있으면)
      items.push({ name: entry.name, path: relativePath, type: 'folder', children });
    } else if (entry.name.endsWith('.md')) {
      const stat = fs.statSync(fullPath);
      items.push({ name: entry.name, path: relativePath, type: 'file', size: stat.size, modified: stat.mtime.toISOString() });
    }
  }

  return items.sort((a, b) => {
    if (a.type !== b.type) return a.type === 'folder' ? -1 : 1;
    return a.name.localeCompare(b.name);
  });
}

app.listen(PORT, () => {
  console.log(`🔥 MDFlare API running at http://localhost:${PORT}`);
  console.log(`📁 Data: ${DATA_DIR}`);
});
