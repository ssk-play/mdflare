const express = require('express');
const cors = require('cors');
const fs = require('fs');
const path = require('path');

const app = express();
const PORT = 3001;

// 유저별 vault 경로 매핑 (테스트: test → test-vault/)
const VAULTS = {
  test: path.join(__dirname, '..', 'test-vault')
};

app.use(cors());
app.use(express.json());

// 파일 트리 가져오기
app.get('/api/:userId/files', (req, res) => {
  const vaultPath = VAULTS[req.params.userId];
  if (!vaultPath) return res.status(404).json({ error: 'User not found' });

  const tree = buildFileTree(vaultPath, vaultPath);
  res.json({ user: req.params.userId, files: tree });
});

// 파일 내용 읽기
app.get('/api/:userId/file/*', (req, res) => {
  const vaultPath = VAULTS[req.params.userId];
  if (!vaultPath) return res.status(404).json({ error: 'User not found' });

  const filePath = path.join(vaultPath, req.params[0]);
  
  // 보안: vault 밖으로 나가는 거 방지
  if (!filePath.startsWith(vaultPath)) {
    return res.status(403).json({ error: 'Access denied' });
  }

  if (!fs.existsSync(filePath)) {
    return res.status(404).json({ error: 'File not found' });
  }

  const content = fs.readFileSync(filePath, 'utf-8');
  const stat = fs.statSync(filePath);
  
  res.json({
    path: req.params[0],
    content,
    size: stat.size,
    modified: stat.mtime.toISOString()
  });
});

// 파일 저장
app.put('/api/:userId/file/*', (req, res) => {
  const vaultPath = VAULTS[req.params.userId];
  if (!vaultPath) return res.status(404).json({ error: 'User not found' });

  const filePath = path.join(vaultPath, req.params[0]);
  
  if (!filePath.startsWith(vaultPath)) {
    return res.status(403).json({ error: 'Access denied' });
  }

  // 디렉토리 자동 생성
  const dir = path.dirname(filePath);
  if (!fs.existsSync(dir)) {
    fs.mkdirSync(dir, { recursive: true });
  }

  fs.writeFileSync(filePath, req.body.content, 'utf-8');
  const stat = fs.statSync(filePath);

  res.json({
    path: req.params[0],
    size: stat.size,
    modified: stat.mtime.toISOString(),
    saved: true
  });
});

// 파일/폴더 이름 변경
app.post('/api/:userId/rename', (req, res) => {
  const vaultPath = VAULTS[req.params.userId];
  if (!vaultPath) return res.status(404).json({ error: 'User not found' });

  const { oldPath, newPath } = req.body;
  const oldFull = path.join(vaultPath, oldPath);
  const newFull = path.join(vaultPath, newPath);

  if (!oldFull.startsWith(vaultPath) || !newFull.startsWith(vaultPath)) {
    return res.status(403).json({ error: 'Access denied' });
  }

  if (!fs.existsSync(oldFull)) {
    return res.status(404).json({ error: 'Not found' });
  }

  const newDir = path.dirname(newFull);
  if (!fs.existsSync(newDir)) {
    fs.mkdirSync(newDir, { recursive: true });
  }

  fs.renameSync(oldFull, newFull);
  res.json({ renamed: true, oldPath, newPath });
});

// 파일 삭제
app.delete('/api/:userId/file/*', (req, res) => {
  const vaultPath = VAULTS[req.params.userId];
  if (!vaultPath) return res.status(404).json({ error: 'User not found' });

  const filePath = path.join(vaultPath, req.params[0]);
  
  if (!filePath.startsWith(vaultPath)) {
    return res.status(403).json({ error: 'Access denied' });
  }

  if (!fs.existsSync(filePath)) {
    return res.status(404).json({ error: 'File not found' });
  }

  fs.unlinkSync(filePath);
  res.json({ deleted: true, path: req.params[0] });
});

// 재귀적 파일 트리 빌드
function buildFileTree(dirPath, rootPath) {
  const items = [];
  const entries = fs.readdirSync(dirPath, { withFileTypes: true });
  
  for (const entry of entries) {
    if (entry.name.startsWith('.')) continue; // 숨김 파일 제외
    
    const fullPath = path.join(dirPath, entry.name);
    const relativePath = path.relative(rootPath, fullPath);
    
    if (entry.isDirectory()) {
      items.push({
        name: entry.name,
        path: relativePath,
        type: 'folder',
        children: buildFileTree(fullPath, rootPath)
      });
    } else if (entry.name.endsWith('.md')) {
      const stat = fs.statSync(fullPath);
      items.push({
        name: entry.name,
        path: relativePath,
        type: 'file',
        size: stat.size,
        modified: stat.mtime.toISOString()
      });
    }
  }
  
  // 폴더 먼저, 그 다음 파일 (이름순)
  return items.sort((a, b) => {
    if (a.type !== b.type) return a.type === 'folder' ? -1 : 1;
    return a.name.localeCompare(b.name);
  });
}

app.listen(PORT, () => {
  console.log(`🔥 MDFlare API running at http://localhost:${PORT}`);
  console.log(`📁 Vault: test → ${VAULTS.test}`);
});
