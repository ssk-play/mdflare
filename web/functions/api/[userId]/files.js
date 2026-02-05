// GET /api/:userId/files — 파일 트리
export async function onRequestGet(context) {
  const { params, env } = context;
  const userId = params.userId;
  const prefix = `${userId}/`;

  try {
    const listed = await env.VAULT.list({ prefix });
    const tree = buildTreeFromR2(listed.objects, prefix);
    return Response.json({ user: userId, files: tree });
  } catch (err) {
    return Response.json({ error: err.message }, { status: 500 });
  }
}

function buildTreeFromR2(objects, prefix) {
  const tree = [];
  const folders = new Map();

  for (const obj of objects) {
    const relativePath = obj.key.substring(prefix.length);
    if (!relativePath || relativePath.endsWith('.gitkeep')) continue;
    if (!relativePath.endsWith('.md')) continue;

    const parts = relativePath.split('/');
    
    if (parts.length === 1) {
      tree.push({
        name: parts[0],
        path: relativePath,
        type: 'file',
        size: obj.size,
        modified: obj.uploaded.toISOString()
      });
    } else {
      // 폴더 구조 구축
      let currentLevel = tree;
      for (let i = 0; i < parts.length - 1; i++) {
        const folderName = parts[i];
        const folderPath = parts.slice(0, i + 1).join('/');
        
        let folder = currentLevel.find(f => f.type === 'folder' && f.name === folderName);
        if (!folder) {
          folder = { name: folderName, path: folderPath, type: 'folder', children: [] };
          currentLevel.push(folder);
        }
        currentLevel = folder.children;
      }
      
      currentLevel.push({
        name: parts[parts.length - 1],
        path: relativePath,
        type: 'file',
        size: obj.size,
        modified: obj.uploaded.toISOString()
      });
    }
  }

  return sortTree(tree);
}

function sortTree(items) {
  items.sort((a, b) => {
    if (a.type !== b.type) return a.type === 'folder' ? -1 : 1;
    return a.name.localeCompare(b.name);
  });
  for (const item of items) {
    if (item.children) sortTree(item.children);
  }
  return items;
}
