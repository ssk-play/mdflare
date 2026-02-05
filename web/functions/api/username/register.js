// POST /api/username/register — username 등록
// body: { uid, username, displayName }
export async function onRequestPost(context) {
  const { env, request } = context;
  const body = await request.json();
  const { uid, username, displayName } = body;

  if (!uid || !username) {
    return Response.json({ error: 'uid and username required' }, { status: 400 });
  }

  const name = username.toLowerCase().trim();

  // 유효성 검사
  if (!/^[a-z0-9][a-z0-9-]{1,18}[a-z0-9]$/.test(name)) {
    return Response.json({ error: 'invalid username format' }, { status: 400 });
  }

  // 이미 등록된 username인지 체크
  const existing = await env.VAULT.get(`_usernames/${name}`);
  if (existing) {
    return Response.json({ error: 'username already taken' }, { status: 409 });
  }

  // 이미 username이 있는 유저인지 체크
  const existingUser = await env.VAULT.get(`_users/${uid}`);
  if (existingUser) {
    return Response.json({ error: 'user already has a username' }, { status: 409 });
  }

  const now = new Date().toISOString();

  // username → uid 매핑
  await env.VAULT.put(`_usernames/${name}`, JSON.stringify({ uid, createdAt: now }));

  // uid → profile 매핑
  await env.VAULT.put(`_users/${uid}`, JSON.stringify({
    username: name,
    displayName: displayName || '',
    createdAt: now
  }));

  return Response.json({ registered: true, username: name });
}
