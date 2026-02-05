// GET /api/:userId/file/:path — 파일 읽기
// PUT /api/:userId/file/:path — 파일 저장
// DELETE /api/:userId/file/:path — 파일 삭제
export async function onRequest(context) {
  const { request, params, env, data } = context;
  const userId = data.resolvedUid || params.userId;
  const filePath = params.path.join('/');
  const r2Key = `${userId}/${filePath}`;

  switch (request.method) {
    case 'GET':
      return handleGet(env, r2Key, filePath);
    case 'PUT':
      return handlePut(env, r2Key, filePath, request);
    case 'DELETE':
      return handleDelete(env, r2Key, filePath);
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

async function handleDelete(env, r2Key, filePath) {
  await env.VAULT.delete(r2Key);
  return Response.json({ deleted: true, path: filePath });
}
