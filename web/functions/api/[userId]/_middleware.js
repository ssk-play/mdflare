// Middleware: username → uid 리졸브 + API 토큰 인증 (쓰기 요청)
export async function onRequest(context) {
  const { env, params, request } = context;
  const username = params.userId;

  // _usernames/{username} → { uid } 매핑 조회
  const obj = await env.VAULT.get(`_usernames/${username}`);
  if (!obj) {
    context.data.resolvedUid = username;
    context.data.isOwner = false;
    return context.next();
  }

  const userData = JSON.parse(await obj.text());
  context.data.resolvedUid = userData.uid;
  context.data.ownerUid = userData.uid;

  // 쓰기 요청(PUT/POST/DELETE)은 인증 필요
  const method = request.method;
  if (method === 'PUT' || method === 'POST' || method === 'DELETE') {
    const authHeader = request.headers.get('Authorization') || '';
    const token = authHeader.replace('Bearer ', '');
    const firebaseUid = request.headers.get('X-Firebase-UID') || '';

    // Firebase UID 직접 전달 (웹 클라이언트)
    if (firebaseUid && firebaseUid === userData.uid) {
      context.data.isOwner = true;
      return context.next();
    }

    // API 토큰 인증 (에이전트)
    if (token) {
      const tokenObj = await env.VAULT.get(`_tokens/${token}`);
      if (tokenObj) {
        const tokenData = JSON.parse(await tokenObj.text());
        if (tokenData.uid === userData.uid) {
          context.data.isOwner = true;
          return context.next();
        }
      }
    }

    // 인증 실패 → 403
    return Response.json({ error: 'Unauthorized' }, { status: 403 });
  }

  // GET 요청은 인증 없이 통과 (공개 읽기)
  context.data.isOwner = false;
  return context.next();
}
