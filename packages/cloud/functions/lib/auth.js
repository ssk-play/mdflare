// Firebase ID Token 검증 (Cloudflare Workers용)
// jose 라이브러리로 JWT 서명 검증

import * as jose from 'jose';

const FIREBASE_PROJECT_ID = 'markdownflare';

// Firebase/Google 공개키 JWKS URL
const JWKS_URL = 'https://www.googleapis.com/service_accounts/v1/jwk/securetoken@system.gserviceaccount.com';

// JWKS 캐시 (Worker 인스턴스별)
let cachedJWKS = null;

function getJWKS() {
  if (!cachedJWKS) {
    cachedJWKS = jose.createRemoteJWKSet(new URL(JWKS_URL));
  }
  return cachedJWKS;
}

// Firebase ID Token 검증
export async function verifyFirebaseToken(token) {
  if (!token) {
    throw new Error('No token provided');
  }

  try {
    const JWKS = getJWKS();
    
    const { payload } = await jose.jwtVerify(token, JWKS, {
      issuer: `https://securetoken.google.com/${FIREBASE_PROJECT_ID}`,
      audience: FIREBASE_PROJECT_ID,
    });

    // 추가 검증
    const now = Math.floor(Date.now() / 1000);

    // subject (uid) 확인
    if (!payload.sub || typeof payload.sub !== 'string') {
      throw new Error('Invalid subject');
    }

    // auth_time 확인
    if (!payload.auth_time || payload.auth_time > now + 300) {
      throw new Error('Invalid auth time');
    }

    return {
      uid: payload.sub,
      email: payload.email || null,
      emailVerified: payload.email_verified || false,
      name: payload.name || null,
      picture: payload.picture || null,
      authTime: payload.auth_time,
      exp: payload.exp,
      iat: payload.iat
    };

  } catch (err) {
    // jose 에러 메시지 정리
    const message = err.code === 'ERR_JWT_EXPIRED' ? 'Token expired'
      : err.code === 'ERR_JWS_SIGNATURE_VERIFICATION_FAILED' ? 'Invalid signature'
      : err.code === 'ERR_JWT_CLAIM_VALIDATION_FAILED' ? 'Invalid claims'
      : err.message;
    
    throw new Error(`Token verification failed: ${message}`);
  }
}

// 인증 헤더에서 토큰 추출
export function extractToken(request) {
  const authHeader = request.headers.get('Authorization') || '';
  if (authHeader.startsWith('Bearer ')) {
    return authHeader.slice(7);
  }
  return null;
}

// API 토큰 검증 (에이전트용)
export async function verifyApiToken(token, env) {
  if (!token) return null;

  const tokenObj = await env.VAULT.get(`_tokens/${token}`);
  if (!tokenObj) return null;

  try {
    const tokenData = JSON.parse(await tokenObj.text());
    return {
      uid: tokenData.uid,
      username: tokenData.username,
      type: tokenData.type || 'api'
    };
  } catch {
    return null;
  }
}
