// POST /api/token/generate — API 토큰 생성
// body: { uid, username }
export async function onRequestPost(context) {
  try {
    const { env, request } = context;
    const body = await request.json();
    const { uid, username } = body;

    if (!uid || !username) {
      return Response.json({ error: 'uid and username required' }, { status: 400 });
    }

    if (!env.VAULT) {
      return Response.json({ error: 'R2 binding not configured' }, { status: 500 });
    }

    // 기존 토큰이 있으면 삭제
    const existingObj = await env.VAULT.get(`_users/${uid}`);
    if (existingObj) {
      try {
        const profile = JSON.parse(await existingObj.text());
        if (profile.apiToken) {
          await env.VAULT.delete(`_tokens/${profile.apiToken}`);
        }
      } catch (e) {
        // 파싱 실패해도 계속 진행
      }
    }

    // 새 토큰 생성 (48자 hex)
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
      try {
        profile = JSON.parse(await existingObj.text());
      } catch (e) {}
    }
    profile.apiToken = token;
    await env.VAULT.put(`_users/${uid}`, JSON.stringify(profile));

    return Response.json({ token });
  } catch (err) {
    return Response.json({ error: err.message || 'Unknown error' }, { status: 500 });
  }
}
