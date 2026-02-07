// POST /api/:userId/rename — 이름 변경

function toSafeKey(filePath) {
  return filePath.replace(/\./g, '_dot_').replace(/\//g, '_slash_');
}

async function writeRtdb(env, username, filePath, data) {
  const secret = env.FIREBASE_DB_SECRET;
  if (!secret) return;
  const safeKey = toSafeKey(filePath);
  const url = `https://markdownflare-default-rtdb.firebaseio.com/mdflare/${username}/files/${safeKey}.json?auth=${secret}`;
  try {
    await fetch(url, {
      method: 'PUT',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(data),
    });
  } catch (e) {
    console.error('RTDB write failed:', e);
  }
}

async function deleteRtdb(env, username, filePath) {
  const secret = env.FIREBASE_DB_SECRET;
  if (!secret) return;
  const safeKey = toSafeKey(filePath);
  const url = `https://markdownflare-default-rtdb.firebaseio.com/mdflare/${username}/files/${safeKey}.json?auth=${secret}`;
  try {
    await fetch(url, { method: 'DELETE' });
  } catch (e) {
    console.error('RTDB delete failed:', e);
  }
}

export async function onRequestPost(context) {
  const { params, env, request, data } = context;
  const userId = data.resolvedUid || params.userId;
  const username = params.userId;
  const body = await request.json();
  const oldPath = decodeURIComponent(body.oldPath);
  const newPath = decodeURIComponent(body.newPath);

  const oldKey = `vaults/${userId}/${oldPath}`;
  const newKey = `vaults/${userId}/${newPath}`;

  try {
    const object = await env.VAULT.get(oldKey);
    if (!object) {
      return Response.json({ error: 'Not found' }, { status: 404 });
    }

    const content = await object.text();
    await env.VAULT.put(newKey, content, {
      customMetadata: object.customMetadata
    });
    await env.VAULT.delete(oldKey);

    // RTDB 업데이트: 이전 경로 삭제 + 새 경로에 rename 기록
    if (data.isOwner && username) {
      await deleteRtdb(env, username, oldPath);
      await writeRtdb(env, username, newPath, {
        path: newPath,
        action: 'rename',
        hash: '',
        modified: Date.now(),
        size: content.length,
        oldPath,
      });
    }

    return Response.json({ renamed: true, oldPath, newPath });
  } catch (err) {
    return Response.json({ error: err.message }, { status: 500 });
  }
}
