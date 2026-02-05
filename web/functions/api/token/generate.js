// POST /api/token/generate — API 토큰 생성
// body: { uid, username }
export async function onRequestPost(context) {
  const { env, request } = context;
  const body = await request.json();
  const { uid, username } = body;

  if (!uid || !username) {
    return Response.json({ error: 'uid and username required' }, { status: 400 });
  }

  // 기존 토큰이 있으면 삭제
  const existingObj = await env.VAULT.get(`_users/${uid}`);
  if (existingObj) {
    const profile = JSON.parse(await existingObj.text());
    if (profile.apiToken) {
      await env.VAULT.delete(`_tokens/${profile.apiToken}`);
    }
  }

  // 새 토큰 생성 (32자 랜덤)
  const token = Array.from(crypto.getRandomValues(new Uint8Array(24)))
    .map(b => b.toString(16).padStart(2, '0')).join('');

  // 토큰 → uid/username 매핑 저장
  await env.VAULT.put(`_tokens/${token}`, JSON.stringify({
    uid,
    username,
    createdAt: new Date().toISOString()
  }));

  // 유저 프로필에 토큰 저장
  let profile = {};
  if (existingObj) {
    profile = JSON.parse(await existingObj.text());
  }
  profile.apiToken = token;
  await env.VAULT.put(`_users/${uid}`, JSON.stringify(profile));

  return Response.json({ token });
}
