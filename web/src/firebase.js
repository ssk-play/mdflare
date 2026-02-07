import { initializeApp } from 'firebase/app';
import { getDatabase, ref, set, remove, onValue } from 'firebase/database';
import { getAuth, GoogleAuthProvider, signInWithPopup, signOut, onAuthStateChanged } from 'firebase/auth';

const firebaseConfig = {
  apiKey: "AIzaSyBfif2L4Cz11AYTP_OEsJZQJrNWe06a6CY",
  authDomain: "markdownflare.firebaseapp.com",
  databaseURL: "https://markdownflare-default-rtdb.firebaseio.com",
  projectId: "markdownflare",
  storageBucket: "markdownflare.firebasestorage.app",
  messagingSenderId: "658985431247",
  appId: "1:658985431247:web:07966fe83f40b1a7de07ba",
  measurementId: "G-VNWSZDGXV8"
};

const app = initializeApp(firebaseConfig);
const db = getDatabase(app);

// RTDB safe key: path → Firebase-safe key
function toSafeKey(filePath) {
  return filePath.replace(/\./g, '_dot_').replace(/\//g, '_slash_');
}

// 파일 메타데이터 업데이트 (모든 파일 변경 시 호출)
export function updateFileMeta(userId, filePath, { size, hash, action = 'save', oldHash, diff, oldPath }) {
  const safeKey = toSafeKey(filePath);
  const fileRef = ref(db, `mdflare/${userId}/files/${safeKey}`);
  const data = {
    path: filePath,
    action,
    hash,
    modified: Date.now(),
    size,
  };
  if (action === 'save' && oldHash) {
    data.oldHash = oldHash;
    if (diff && JSON.stringify(diff).length <= 10240) {
      data.diff = diff;
    }
  }
  if (action === 'rename' && oldPath) {
    data.oldPath = oldPath;
  }
  return set(fileRef, data);
}

// 파일 RTDB 엔트리 삭제 (파일 삭제 또는 rename 시 이전 경로 정리)
export function deleteFileMeta(userId, filePath) {
  const safeKey = toSafeKey(filePath);
  const fileRef = ref(db, `mdflare/${userId}/files/${safeKey}`);
  return remove(fileRef);
}

// 파일 변경 리스너 (실시간 감지)
export function onFilesChanged(userId, callback) {
  const filesRef = ref(db, `mdflare/${userId}/files`);
  return onValue(filesRef, (snapshot) => {
    const data = snapshot.val();
    if (data) {
      callback(Object.values(data));
    }
  });
}

// 간단한 해시 생성
export function simpleHash(str) {
  let hash = 0;
  for (let i = 0; i < str.length; i++) {
    const char = str.charCodeAt(i);
    hash = ((hash << 5) - hash) + char;
    hash |= 0;
  }
  return hash.toString(36);
}

// 라인 기반 diff 생성 (LCS)
export function computeLineDiff(oldText, newText) {
  const oldLines = oldText.split('\n');
  const newLines = newText.split('\n');
  const m = oldLines.length;
  const n = newLines.length;

  // LCS DP (메모리 최적화: 2행만 유지)
  let prev = new Uint16Array(n + 1);
  let curr = new Uint16Array(n + 1);
  for (let i = 1; i <= m; i++) {
    [prev, curr] = [curr, prev];
    curr.fill(0);
    for (let j = 1; j <= n; j++) {
      if (oldLines[i - 1] === newLines[j - 1]) {
        curr[j] = prev[j - 1] + 1;
      } else {
        curr[j] = Math.max(prev[j], curr[j - 1]);
      }
    }
  }

  // LCS 역추적 (full DP 필요)
  const dp = Array.from({ length: m + 1 }, () => new Uint16Array(n + 1));
  for (let i = 1; i <= m; i++) {
    for (let j = 1; j <= n; j++) {
      if (oldLines[i - 1] === newLines[j - 1]) {
        dp[i][j] = dp[i - 1][j - 1] + 1;
      } else {
        dp[i][j] = Math.max(dp[i - 1][j], dp[i][j - 1]);
      }
    }
  }

  // 역추적으로 edit script 생성
  const ops = []; // {type: 'eq'|'del'|'ins', lines?: string[]}
  let i = m, j = n;
  const raw = [];
  while (i > 0 || j > 0) {
    if (i > 0 && j > 0 && oldLines[i - 1] === newLines[j - 1]) {
      raw.push({ type: 'eq' });
      i--; j--;
    } else if (j > 0 && (i === 0 || dp[i][j - 1] >= dp[i - 1][j])) {
      raw.push({ type: 'ins', line: newLines[j - 1] });
      j--;
    } else {
      raw.push({ type: 'del' });
      i--;
    }
  }
  raw.reverse();

  // 연속된 같은 타입을 합치기
  const diff = [];
  let eqCount = 0;
  let delCount = 0;
  let insLines = [];

  function flush() {
    if (eqCount > 0) { diff.push({ eq: eqCount }); eqCount = 0; }
    if (delCount > 0) { diff.push({ del: delCount }); delCount = 0; }
    if (insLines.length > 0) { diff.push({ ins: insLines }); insLines = []; }
  }

  for (const r of raw) {
    if (r.type === 'eq') {
      if (delCount > 0 || insLines.length > 0) {
        // del/ins 먼저 flush
        if (delCount > 0) { diff.push({ del: delCount }); delCount = 0; }
        if (insLines.length > 0) { diff.push({ ins: insLines }); insLines = []; }
      }
      eqCount++;
    } else if (r.type === 'del') {
      if (eqCount > 0) { diff.push({ eq: eqCount }); eqCount = 0; }
      if (insLines.length > 0) { diff.push({ ins: insLines }); insLines = []; }
      delCount++;
    } else { // ins
      if (eqCount > 0) { diff.push({ eq: eqCount }); eqCount = 0; }
      insLines.push(r.line);
    }
  }
  flush();

  return diff;
}

// Auth
const auth = getAuth(app);
const googleProvider = new GoogleAuthProvider();

export function loginWithGoogle() {
  return signInWithPopup(auth, googleProvider);
}

export function logout() {
  return signOut(auth);
}

export function onAuthChange(callback) {
  return onAuthStateChanged(auth, callback);
}

export { db, auth, googleProvider };
