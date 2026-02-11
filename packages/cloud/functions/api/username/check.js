// GET /api/username/check?name=xxx — username 중복 체크
export async function onRequestGet(context) {
  const { env, request } = context;
  const url = new URL(request.url);
  const name = url.searchParams.get('name');

  if (!name) {
    return Response.json({ error: 'name required' }, { status: 400 });
  }

  const username = name.toLowerCase().trim();

  // 유효성 검사: 3-20자, 영문소문자+숫자+하이픈, 시작/끝은 영문숫자
  if (!/^[a-z0-9][a-z0-9-]{1,18}[a-z0-9]$/.test(username)) {
    return Response.json({ available: false, reason: '3-20자, 영문소문자/숫자/하이픈만 가능 (시작과 끝은 영문 또는 숫자)' });
  }

  // 예약어 체크
  const reserved = ['admin', 'api', 'www', 'app', 'help', 'support', 'login', 'signup', 'settings', 'profile', 'username'];
  if (reserved.includes(username)) {
    return Response.json({ available: false, reason: '예약된 이름입니다' });
  }

  const existing = await env.VAULT.get(`_usernames/${username}`);
  return Response.json({ available: !existing, username });
}
