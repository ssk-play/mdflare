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
      return handleGet(env, r2Key, filePath);
    case 'PUT':
      return handlePut(env, r2Key, filePath, request);
    case 'DELETE':
      return handleDelete(env, r2Key, filePath, request);
    default:
      return Response.json({ error: 'Method not allowed' }, { status: 405 });
  }
}

async function handleGet(env, r2Key, filePath) {
  const object = await env.VAULT.get(r2Key);
  if (!object) {
    return Response.json({ error: 'File not found' }, { status: 404 });
  }

  const content = await object.text();
  return Response.json({
    path: filePath,
    content,
    size: object.size,
    modified: object.uploaded.toISOString()
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
