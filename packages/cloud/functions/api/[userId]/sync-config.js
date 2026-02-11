// GET /api/:userId/sync-config — 에이전트용 RTDB 접속 정보
export async function onRequestGet(context) {
  const { params, env, data } = context;
  const username = params.userId;

  // 소유자 인증 필수
  if (!data.isOwner) {
    return Response.json({ error: 'Unauthorized' }, { status: 403 });
  }

  const rtdbUrl = 'https://markdownflare-default-rtdb.firebaseio.com';
  const rtdbAuth = env.FIREBASE_DB_SECRET || '';

  if (!rtdbAuth) {
    return Response.json({ error: 'RTDB not configured' }, { status: 500 });
  }

  return Response.json({
    rtdbUrl,
    rtdbAuth,
    userId: username,
  });
}
