// GET /api/:userId/file/:path — 파일 읽기
// PUT /api/:userId/file/:path — 파일 저장
// DELETE /api/:userId/file/:path — 파일 삭제
export async function onRequest(context) {
  const { request, params, env, data } = context;
  const userId = data.resolvedUid || params.userId;
  const username = params.userId;
  const filePath = decodeURIComponent(params.path.join('/'));
  const r2Key = `vaults/${userId}/${filePath}`;

  switch (request.method) {
    case 'GET':
      return handleGet(env, r2Key, filePath, userId, data);
    case 'PUT':
      return handlePut(env, r2Key, filePath, request, username, data);
    case 'DELETE':
      return handleDelete(env, r2Key, filePath, request, username, data);
    default:
      return Response.json({ error: 'Method not allowed' }, { status: 405 });
  }
}

// Firebase RTDB helper
function toSafeKey(filePath) {
  return filePath.replace(/\./g, '_dot_').replace(/\//g, '_slash_');
}

function simpleHash(str) {
  let hash = 0;
  for (let i = 0; i < str.length; i++) {
    const char = str.charCodeAt(i);
    hash = ((hash << 5) - hash) + char;
    hash |= 0;
  }
  return hash.toString(36);
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

// 파일 공개 여부 확인
async function isFilePublic(env, userId, filePath) {
  // 1. 파일별 공개 설정 확인
  const metaKey = `_meta/${userId}/public/${filePath}`;
  const meta = await env.VAULT.get(metaKey);
  if (meta) {
    try {
      const data = JSON.parse(await meta.text());
      if (data.public === true) return true;
    } catch {}
  }
  
  // 2. 상위 폴더 공개 설정 확인 (폴더 전체 공개)
  const parts = filePath.split('/');
  for (let i = parts.length - 1; i >= 0; i--) {
    const folderPath = parts.slice(0, i).join('/') || '_root';
    const folderMetaKey = `_meta/${userId}/public/${folderPath}/_folder`;
    const folderMeta = await env.VAULT.get(folderMetaKey);
    if (folderMeta) {
      try {
        const data = JSON.parse(await folderMeta.text());
        if (data.public === true) return true;
      } catch {}
    }
  }
  
  return false;
}

async function handleGet(env, r2Key, filePath, userId, data) {
  // 권한 체크: 소유자가 아니면 공개 파일인지 확인
  if (data.needsPublicCheck) {
    const isPublic = await isFilePublic(env, userId, filePath);
    if (!isPublic) {
      return Response.json({ error: 'Access denied' }, { status: 403 });
    }
    data.isPublic = true;
  }

  const object = await env.VAULT.get(r2Key);
  if (!object) {
    return Response.json({ error: 'File not found' }, { status: 404 });
  }

  const content = await object.text();
  return Response.json({
    path: filePath,
    content,
    size: object.size,
    modified: object.uploaded.toISOString(),
    public: data.isPublic || false
  });
}

async function handlePut(env, r2Key, filePath, request, username, data) {
  const body = await request.json();
  const content = body.content;
  const size = new Blob([content]).size;
  const modified = new Date().toISOString();

  await env.VAULT.put(r2Key, content, {
    customMetadata: { modified }
  });

  // 에이전트 업로드 시 RTDB 기록 (isOwner = API 토큰 인증된 소유자)
  if (data.isOwner && username) {
    const hash = simpleHash(content);
    const rtdbData = {
      path: filePath,
      action: body.oldHash ? 'save' : 'create',
      hash,
      modified: Date.now(),
      size,
    };
    if (body.oldHash) {
      rtdbData.oldHash = body.oldHash;
      if (body.diff && JSON.stringify(body.diff).length <= 10240) {
        rtdbData.diff = body.diff;
      }
    }
    await writeRtdb(env, username, filePath, rtdbData);
  }

  return Response.json({
    path: filePath,
    size,
    modified,
    saved: true
  });
}

async function handleDelete(env, r2Key, filePath, request, username, data) {
  // 폴더 삭제: ?folder=true 쿼리 파라미터
  const url = new URL(request.url);
  const isFolder = url.searchParams.get('folder') === 'true';

  if (isFolder) {
    // R2에서 해당 prefix 아래 모든 오브젝트 삭제
    const prefix = r2Key.endsWith('/') ? r2Key : `${r2Key}/`;
    const listed = await env.VAULT.list({ prefix });
    const keys = listed.objects.map(obj => obj.key);

    if (keys.length === 0) {
      return Response.json({ deleted: true, path: filePath, count: 0 });
    }

    // R2 delete는 배열 지원 (최대 1000개씩)
    await env.VAULT.delete(keys);

    // 폴더 내 각 파일의 RTDB 엔트리 삭제
    if (data.isOwner && username) {
      const vaultPrefix = `vaults/${data.resolvedUid}/`;
      for (const key of keys) {
        const fp = key.startsWith(vaultPrefix) ? key.slice(vaultPrefix.length) : key;
        await deleteRtdb(env, username, fp);
      }
    }

    return Response.json({ deleted: true, path: filePath, count: keys.length });
  }

  await env.VAULT.delete(r2Key);

  if (data.isOwner && username) {
    await deleteRtdb(env, username, filePath);
  }

  return Response.json({ deleted: true, path: filePath });
}
