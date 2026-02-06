// GET /api/:userId/share/:path — 공개 상태 조회
// PUT /api/:userId/share/:path — 공개 설정
// DELETE /api/:userId/share/:path — 공개 해제
export async function onRequest(context) {
  const { request, params, env, data } = context;
  const userId = data.resolvedUid || params.userId;
  const filePath = decodeURIComponent(params.path.join('/'));
  
  // 소유자만 공유 설정 가능
  if (!data.isOwner) {
    return Response.json({ error: 'Unauthorized' }, { status: 403 });
  }

  const metaKey = `_meta/${userId}/public/${filePath}`;

  switch (request.method) {
    case 'GET':
      return handleGet(env, metaKey, filePath);
    case 'PUT':
      return handlePut(env, metaKey, filePath, request);
    case 'DELETE':
      return handleDelete(env, metaKey, filePath);
    default:
      return Response.json({ error: 'Method not allowed' }, { status: 405 });
  }
}

async function handleGet(env, metaKey, filePath) {
  const meta = await env.VAULT.get(metaKey);
  if (!meta) {
    return Response.json({ path: filePath, public: false });
  }
  
  try {
    const data = JSON.parse(await meta.text());
    return Response.json({ path: filePath, public: data.public || false, sharedAt: data.sharedAt });
  } catch {
    return Response.json({ path: filePath, public: false });
  }
}

async function handlePut(env, metaKey, filePath, request) {
  const body = await request.json().catch(() => ({}));
  const isPublic = body.public !== false; // 기본값 true
  
  await env.VAULT.put(metaKey, JSON.stringify({
    public: isPublic,
    sharedAt: new Date().toISOString()
  }));
  
  return Response.json({ path: filePath, public: isPublic, sharedAt: new Date().toISOString() });
}

async function handleDelete(env, metaKey, filePath) {
  await env.VAULT.delete(metaKey);
  return Response.json({ path: filePath, public: false });
}
