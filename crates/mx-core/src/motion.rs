use crate::buffer::Buffer;

pub fn forward_char(buffer: &Buffer, from: usize, count: usize) -> usize {
    from.saturating_add(count).min(buffer.len_chars())
}

pub fn backward_char(from: usize, count: usize) -> usize {
    from.saturating_sub(count)
}

pub fn beginning_of_line(buffer: &Buffer, from: usize) -> usize {
    buffer.line_start(buffer.line_of(from))
}

pub fn end_of_line(buffer: &Buffer, from: usize) -> usize {
    buffer.line_end(buffer.line_of(from))
}

pub fn vertical(buffer: &Buffer, from: usize, lines: isize, goal_column: usize) -> usize {
    let Some(target) = buffer.line_of(from).checked_add_signed(lines) else {
        return 0;
    };
    if target >= buffer.len_lines() {
        return buffer.len_chars();
    }
    (buffer.line_start(target) + goal_column).min(buffer.line_end(target))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forward_char_stops_at_end_of_buffer() {
        let buffer = Buffer::with_text("t", "abc");

        assert_eq!(forward_char(&buffer, 1, 1), 2);
        assert_eq!(forward_char(&buffer, 1, 10), 3);
    }

    #[test]
    fn backward_char_stops_at_start_of_buffer() {
        assert_eq!(backward_char(2, 1), 1);
        assert_eq!(backward_char(2, 10), 0);
    }

    #[test]
    fn line_beginning_and_end_bound_the_current_line() {
        let buffer = Buffer::with_text("t", "ab\ncde\nf");

        assert_eq!(beginning_of_line(&buffer, 5), 3);
        assert_eq!(end_of_line(&buffer, 4), 6);
        assert_eq!(end_of_line(&buffer, 8), 8);
    }

    #[test]
    fn vertical_keeps_goal_column() {
        let buffer = Buffer::with_text("t", "abc\nde\nfghi");

        assert_eq!(vertical(&buffer, 2, 2, 2), 9);
        assert_eq!(vertical(&buffer, 9, -2, 2), 2);
    }

    #[test]
    fn vertical_clamps_goal_column_to_line_end() {
        let buffer = Buffer::with_text("t", "abc\nde\nfghi");

        assert_eq!(vertical(&buffer, 3, 1, 3), 6);
    }

    #[test]
    fn vertical_past_last_line_moves_to_end_of_buffer() {
        let buffer = Buffer::with_text("t", "abc\nde");

        assert_eq!(vertical(&buffer, 4, 1, 0), 6);
        assert_eq!(vertical(&buffer, 0, 5, 0), 6);
    }

    #[test]
    fn vertical_before_first_line_moves_to_start_of_buffer() {
        let buffer = Buffer::with_text("t", "abc\nde");

        assert_eq!(vertical(&buffer, 2, -1, 2), 0);
        assert_eq!(vertical(&buffer, 5, -3, 1), 0);
    }
}
