// POST /api/token/agent — 에이전트용 토큰 생성 (인증 필요)
// body: { uid, username }
import { verifyFirebaseToken, extractToken } from '../../lib/auth.js';

export async function onRequestPost(context) {
  try {
    const { env, request } = context;
    
    // Firebase ID Token 검증
    const idToken = extractToken(request);
    if (!idToken) {
      return Response.json({ error: 'Authentication required' }, { status: 401 });
    }
    
    let decoded;
    try {
      decoded = await verifyFirebaseToken(idToken);
    } catch (e) {
      return Response.json({ error: 'Invalid token' }, { status: 401 });
    }
    
    const body = await request.json();
    const { uid, username } = body;

    if (!uid || !username) {
      return Response.json({ error: 'uid and username required' }, { status: 400 });
    }
    
    // 본인 계정만 토큰 생성 가능
    if (decoded.uid !== uid) {
      return Response.json({ error: 'Cannot generate token for another user' }, { status: 403 });
    }

    if (!env.VAULT) {
      return Response.json({ error: 'R2 binding not configured' }, { status: 500 });
    }

    // 새 토큰 생성 (48자 hex)
    const token = 'agent_' + Array.from(crypto.getRandomValues(new Uint8Array(24)))
      .map(b => b.toString(16).padStart(2, '0')).join('');

    // 토큰 → uid/username 매핑 저장
    await env.VAULT.put(`_tokens/${token}`, JSON.stringify({
      uid,
      username,
      type: 'agent',
      createdAt: new Date().toISOString()
    }));

    // 유저 프로필에 에이전트 토큰 추가 (기존 apiToken은 유지)
    let profile = {};
    const existingObj = await env.VAULT.get(`_users/${uid}`);
    if (existingObj) {
      try {
        profile = JSON.parse(await existingObj.text());
      } catch (e) {}
    }
    
    // agentTokens 배열에 추가 (최대 5개 유지)
    if (!profile.agentTokens) profile.agentTokens = [];
    profile.agentTokens.unshift({
      token,
      createdAt: new Date().toISOString()
    });
    profile.agentTokens = profile.agentTokens.slice(0, 5); // 최근 5개만 유지
    
    await env.VAULT.put(`_users/${uid}`, JSON.stringify(profile));

    return Response.json({ token, username });
  } catch (err) {
    return Response.json({ error: err.message || 'Unknown error' }, { status: 500 });
  }
}
