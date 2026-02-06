// GET /api/:userId/file/:path — 파일 읽기
// PUT /api/:userId/file/:path — 파일 저장
// DELETE /api/:userId/file/:path — 파일 삭제
export async function onRequest(context) {
  const { request, params, env, data } = context;
  const userId = data.resolvedUid || params.userId;
  const filePath = decodeURIComponent(params.path.join('/'));
  const r2Key = `${userId}/${filePath}`;

  switch (request.method) {
    case 'GET':
      return handleGet(env, r2Key, filePath, userId, data);
    case 'PUT':
      return handlePut(env, r2Key, filePath, request);
    case 'DELETE':
      return handleDelete(env, r2Key, filePath, request);
    default:
      return Response.json({ error: 'Method not allowed' }, { status: 405 });
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

async function handlePut(env, r2Key, filePath, request) {
  const body = await request.json();
  const content = body.content;

  await env.VAULT.put(r2Key, content, {
    customMetadata: { modified: new Date().toISOString() }
  });

  return Response.json({
    path: filePath,
    size: new Blob([content]).size,
    modified: new Date().toISOString(),
    saved: true
  });
}

async function handleDelete(env, r2Key, filePath, request) {
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
    return Response.json({ deleted: true, path: filePath, count: keys.length });
  }

  await env.VAULT.delete(r2Key);
  return Response.json({ deleted: true, path: filePath });
}
