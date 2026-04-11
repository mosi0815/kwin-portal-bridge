pub fn rects_intersect(ax: i32, ay: i32, aw: i32, ah: i32, bx: i32, by: i32, bw: i32, bh: i32) -> bool {
    if aw <= 0 || ah <= 0 || bw <= 0 || bh <= 0 {
        return false;
    }

    let a_right = ax.saturating_add(aw);
    let a_bottom = ay.saturating_add(ah);
    let b_right = bx.saturating_add(bw);
    let b_bottom = by.saturating_add(bh);

    ax < b_right && a_right > bx && ay < b_bottom && a_bottom > by
}