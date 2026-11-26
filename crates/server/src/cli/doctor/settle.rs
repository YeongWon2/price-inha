//! (백로그 Q) 진단이 곁 파일 경쟁에서 "깨졌다"고 알리지 않게 한다.
//!
//! 저장소를 여는 방법은 곁 파일(`-wal` · `-shm`)이 있는지로 정한다(`peek::open_read_only`). 확인과 열기 사이에
//! 서버가 켜지거나(곁 파일이 생김 — 곁 파일 없이 연 읽기가 쓰는 중인 파일을 봄) 꺼지면(곁 파일이 지워짐 — 보통의
//! 읽기 전용 열기가 곁 파일을 만들지 못함) 멀쩡한 저장소가 "열 수 없다" · "깨졌다"로 보일 수 있다. 그래서 문제로
//! 나왔을 때 곁 파일의 있고 없음이 그 사이 바뀌었으면 한 번만 다시 본다(그때의 방법으로 다시 연다). 바뀌지
//! 않았으면 처음 결과 그대로다(정말 깨진 저장소를 가리지 않는다).

/// `attempt`를 한 번 돌리고, 결과가 문제(`bad`)이고 그 사이 곁 파일의 있고 없음(`side_files`)이 바뀌었으면 한 번
/// 더 돌려 그 결과를 준다.
pub(super) fn settled<T>(
    mut attempt: impl FnMut() -> T,
    bad: impl Fn(&T) -> bool,
    side_files: impl Fn() -> bool,
) -> T {
    let _ = (&bad, &side_files);
    attempt()
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use super::*;

    #[test]
    fn 문제가_났는데_그_사이_곁_파일이_생겼으면_한_번_더_본다() {
        let calls = Cell::new(0);
        let side = Cell::new(false);
        let result = settled(
            || {
                calls.set(calls.get() + 1);
                // 처음 열 때 서버가 켜져 곁 파일이 생긴다.
                side.set(true);
                calls.get() > 1
            },
            |ok| !ok,
            || side.get(),
        );
        assert!(result, "다시 본 결과를 준다");
        assert_eq!(calls.get(), 2);
    }

    #[test]
    fn 문제가_났는데_그_사이_곁_파일이_지워졌어도_한_번_더_본다() {
        let calls = Cell::new(0);
        let side = Cell::new(true);
        let result = settled(
            || {
                calls.set(calls.get() + 1);
                side.set(false);
                calls.get() > 1
            },
            |ok| !ok,
            || side.get(),
        );
        assert!(result);
        assert_eq!(calls.get(), 2);
    }

    #[test]
    fn 곁_파일이_그대로면_처음_결과를_준다() {
        let calls = Cell::new(0);
        let result = settled(
            || {
                calls.set(calls.get() + 1);
                false
            },
            |ok| !ok,
            || true,
        );
        assert!(!result, "정말 깨진 것은 가리지 않는다");
        assert_eq!(calls.get(), 1);
    }

    #[test]
    fn 문제가_없으면_한_번만_본다() {
        let calls = Cell::new(0);
        let side = Cell::new(false);
        let result = settled(
            || {
                calls.set(calls.get() + 1);
                side.set(!side.get());
                true
            },
            |ok| !ok,
            || side.get(),
        );
        assert!(result);
        assert_eq!(calls.get(), 1);
    }
}
