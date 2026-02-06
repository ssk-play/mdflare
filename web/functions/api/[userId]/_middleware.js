// Middleware: username → uid 리졸브 + 인증 (모든 요청)
import { verifyFirebaseToken, extractToken, verifyApiToken } from '../../lib/auth.js';

export async function onRequest(context) {
  const { env, params, request } = context;
  const username = params.userId;
  const method = request.method;

  // 정적 라우트는 미들웨어 스킵 (username, token 등)
  const staticRoutes = ['username', 'token'];
  if (staticRoutes.includes(username)) {
    return context.next();
  }

  // _usernames/{username} → { uid } 매핑 조회
  const obj = await env.VAULT.get(`_usernames/${username}`);
  if (!obj) {
    // username 없으면 uid 직접 사용 (하위 호환)
    context.data.resolvedUid = username;
    context.data.isOwner = false;
    context.data.isPublic = false;
    
    // 미등록 사용자는 모든 요청 거부
    return Response.json({ error: 'User not found' }, { status: 404 });
  }

  const userData = JSON.parse(await obj.text());
  context.data.resolvedUid = userData.uid;
  context.data.ownerUid = userData.uid;
  context.data.isOwner = false;
  context.data.isPublic = false;

  // 인증 시도 (Firebase ID Token 또는 API Token)
  const token = extractToken(request);
  
  if (token) {
    // 1. Firebase ID Token 검증 시도
    try {
      const decoded = await verifyFirebaseToken(token);
      if (decoded.uid === userData.uid) {
        context.data.isOwner = true;
        context.data.authUser = decoded;
        return context.next();
      }
      // 다른 사용자의 유효한 토큰 → 인증됨, 하지만 소유자 아님
      context.data.authUser = decoded;
    } catch (e) {
      // Firebase 토큰 아님 → API 토큰 시도
      const apiUser = await verifyApiToken(token, env);
      if (apiUser && apiUser.uid === userData.uid) {
        context.data.isOwner = true;
        context.data.authUser = apiUser;
        return context.next();
      }
    }
  }

  // 쓰기 요청(PUT/POST/DELETE)은 반드시 소유자만
  if (method === 'PUT' || method === 'POST' || method === 'DELETE') {
    if (!context.data.isOwner) {
      return Response.json({ error: 'Unauthorized' }, { status: 403 });
    }
  }

  // GET 요청: 소유자가 아니면 공개 파일인지 확인 필요
  // → 실제 파일 핸들러에서 공개 여부 체크
  if (method === 'GET' && !context.data.isOwner) {
    // 공개 체크는 파일 핸들러에서 수행 (경로 정보 필요)
    context.data.needsPublicCheck = true;
  }

  return context.next();
}
