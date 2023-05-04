// HACK: for rust 1.64 (1.68 doesn't need this since this is in lib.rs)
//
// Wanting consistency in our calls
#![allow(clippy::write_with_newline)]

// Std
use std::borrow::Cow;
use std::cmp;
use std::usize;

// Internal
use crate::builder::PossibleValue;
use crate::builder::Str;
use crate::builder::StyledStr;
use crate::builder::Styles;
use crate::builder::{Arg, Command};
use crate::output::display_width;
use crate::output::wrap;
use crate::output::Usage;
use crate::output::TAB;
use crate::output::TAB_WIDTH;
use crate::util::FlatSet;

/// `clap` auto-generated help writer
pub(crate) struct AutoHelp<'cmd, 'writer> {
    template: HelpTemplate<'cmd, 'writer>,
}

// Public Functions
impl<'cmd, 'writer> AutoHelp<'cmd, 'writer> {
    /// Create a new `HelpTemplate` instance.
    pub(crate) fn new(
        writer: &'writer mut StyledStr,
        cmd: &'cmd Command,
        usage: &'cmd Usage<'cmd>,
        use_long: bool,
    ) -> Self {
        Self {
            template: HelpTemplate::new(writer, cmd, usage, use_long),
        }
    }

    pub(crate) fn write_help(&mut self) {
        let pos = self
            .template
            .cmd
            .get_positionals()
            .any(|arg| should_show_arg(self.template.use_long, arg));
        let non_pos = self
            .template
            .cmd
            .get_non_positionals()
            .any(|arg| should_show_arg(self.template.use_long, arg));
        let subcmds = self.template.cmd.has_visible_subcommands();

        let template = if non_pos || pos || subcmds {
            DEFAULT_TEMPLATE
        } else {
            DEFAULT_NO_ARGS_TEMPLATE
        };
        self.template.write_templated_help(template);
    }
}

const DEFAULT_TEMPLATE: &str = "\
{before-help}{about-with-newline}
{usage-heading} {usage}

{all-args}{after-help}\
    ";

const DEFAULT_NO_ARGS_TEMPLATE: &str = "\
{before-help}{about-with-newline}
{usage-heading} {usage}{after-help}\
    ";

/// `clap` HelpTemplate Writer.
///
/// Wraps a writer stream providing different methods to generate help for `clap` objects.
pub(crate) struct HelpTemplate<'cmd, 'writer> {
    writer: &'writer mut StyledStr,
    cmd: &'cmd Command,
    styles: &'cmd Styles,
    usage: &'cmd Usage<'cmd>,
    next_line_help: bool,
    term_w: usize,
    use_long: bool,
}

// Public Functions
impl<'cmd, 'writer> HelpTemplate<'cmd, 'writer> {
    /// Create a new `HelpTemplate` instance.
    pub(crate) fn new(
        writer: &'writer mut StyledStr,
        cmd: &'cmd Command,
        usage: &'cmd Usage<'cmd>,
        use_long: bool,
    ) -> Self {
        debug!(
            "HelpTemplate::new cmd={}, use_long={}",
            cmd.get_name(),
            use_long
        );
        let term_w = match cmd.get_term_width() {
            Some(0) => usize::MAX,
            Some(w) => w,
            None => {
                let (current_width, _h) = dimensions();
                let current_width = current_width.unwrap_or(100);
                let max_width = match cmd.get_max_term_width() {
                    None | Some(0) => usize::MAX,
                    Some(mw) => mw,
                };
                cmp::min(current_width, max_width)
            }
        };
        let next_line_help = cmd.is_next_line_help_set();

        HelpTemplate {
            writer,
            cmd,
            styles: cmd.get_styles(),
            usage,
            next_line_help,
            term_w,
            use_long,
        }
    }

    /// Write help to stream for the parser in the format defined by the template.
    ///
    /// For details about the template language see [`Command::help_template`].
    ///
    /// [`Command::help_template`]: Command::help_template()
    pub(crate) fn write_templated_help(&mut self, template: &str) {
        debug!("HelpTemplate::write_templated_help");
        use std::fmt::Write as _;

        let mut parts = template.split('{');
        if let Some(first) = parts.next() {
            self.writer.push_str(first);
        }
        for part in parts {
            if let Some((tag, rest)) = part.split_once('}') {
                match tag {
                    "name" => {
                        self.write_display_name();
                    }
                    #[cfg(not(feature = "unstable-v5"))]
                    "bin" => {
                        self.write_bin_name();
                    }
                    "version" => {
                        self.write_version();
                    }
                    "author" => {
                        self.write_author(false, false);
                    }
                    "author-with-newline" => {
                        self.write_author(false, true);
                    }
                    "author-section" => {
                        self.write_author(true, true);
                    }
                    "about" => {
                        self.write_about(false, false);
                    }
                    "about-with-newline" => {
                        self.write_about(false, true);
                    }
                    "about-section" => {
                        self.write_about(true, true);
                    }
                    "usage-heading" => {
                        let _ = write!(
                            self.writer,
                            "{}Usage:{}",
                            self.styles.get_usage().render(),
                            self.styles.get_usage().render_reset()
                        );
                    }
                    "usage" => {
                        self.writer.push_styled(
                            &self.usage.create_usage_no_title(&[]).unwrap_or_default(),
                        );
                    }
                    "all-args" => {
                        self.write_all_args();
                    }
                    "options" => {
                        // Include even those with a heading as we don't have a good way of
                        // handling help_heading in the template.
                        self.write_args(
                            &self.cmd.get_non_positionals().collect::<Vec<_>>(),
                            "options",
                            option_sort_key,
                        );
                    }
                    "positionals" => {
                        self.write_args(
                            &self.cmd.get_positionals().collect::<Vec<_>>(),
                            "positionals",
                            positional_sort_key,
                        );
                    }
                    "subcommands" => {
                        self.write_subcommands(self.cmd);
                    }
                    "tab" => {
                        self.writer.push_str(TAB);
                    }
                    "after-help" => {
                        self.write_after_help();
                    }
                    "before-help" => {
                        self.write_before_help();
                    }
                    _ => {
                        let _ = write!(self.writer, "{{{tag}}}");
                    }
                }
                self.writer.push_str(rest);
            }
        }
    }
}

/// Basic template methods
impl<'cmd, 'writer> HelpTemplate<'cmd, 'writer> {
    /// Writes binary name of a Parser Object to the wrapped stream.
    fn write_display_name(&mut self) {
        debug!("HelpTemplate::write_display_name");

        let display_name = wrap(
            &self
                .cmd
                .get_display_name()
                .unwrap_or_else(|| self.cmd.get_name())
                .replace("{n}", "\n"),
            self.term_w,
        );
        self.writer.push_string(display_name);
    }

    /// Writes binary name of a Parser Object to the wrapped stream.
    #[cfg(not(feature = "unstable-v5"))]
    fn write_bin_name(&mut self) {
        debug!("HelpTemplate::write_bin_name");

        let bin_name = if let Some(bn) = self.cmd.get_bin_name() {
            if bn.contains(' ') {
                // In case we're dealing with subcommands i.e. git mv is translated to git-mv
                bn.replace(' ', "-")
            } else {
                wrap(&self.cmd.get_name().replace("{n}", "\n"), self.term_w)
            }
        } else {
            wrap(&self.cmd.get_name().replace("{n}", "\n"), self.term_w)
        };
        self.writer.push_string(bin_name);
    }

    fn write_version(&mut self) {
        let version = self
            .cmd
            .get_version()
            .or_else(|| self.cmd.get_long_version());
        if let Some(output) = version {
            self.writer.push_string(wrap(output, self.term_w));
        }
    }

    fn write_author(&mut self, before_new_line: bool, after_new_line: bool) {
        if let Some(author) = self.cmd.get_author() {
            if before_new_line {
                self.writer.push_str("\n");
            }
            self.writer.push_string(wrap(author, self.term_w));
            if after_new_line {
                self.writer.push_str("\n");
            }
        }
    }

    fn write_about(&mut self, before_new_line: bool, after_new_line: bool) {
        let about = if self.use_long {
            self.cmd.get_long_about().or_else(|| self.cmd.get_about())
        } else {
            self.cmd.get_about()
        };
        if let Some(output) = about {
            if before_new_line {
                self.writer.push_str("\n");
            }
            let mut output = output.clone();
            output.replace_newline_var();
            output.wrap(self.term_w);
            self.writer.push_styled(&output);
            if after_new_line {
                self.writer.push_str("\n");
            }
        }
    }

    fn write_before_help(&mut self) {
        debug!("HelpTemplate::write_before_help");
        let before_help = if self.use_long {
            self.cmd
                .get_before_long_help()
                .or_else(|| self.cmd.get_before_help())
        } else {
            self.cmd.get_before_help()
        };
        if let Some(output) = before_help {
            let mut output = output.clone();
            output.replace_newline_var();
            output.wrap(self.term_w);
            self.writer.push_styled(&output);
            self.writer.push_str("\n\n");
        }
    }

    fn write_after_help(&mut self) {
        debug!("HelpTemplate::write_after_help");
        let after_help = if self.use_long {
            self.cmd
                .get_after_long_help()
                .or_else(|| self.cmd.get_after_help())
        } else {
            self.cmd.get_after_help()
        };
        if let Some(output) = after_help {
            self.writer.push_str("\n\n");
            let mut output = output.clone();
            output.replace_newline_var();
            output.wrap(self.term_w);
            self.writer.push_styled(&output);
        }
    }
}

/// Arg handling
impl<'cmd, 'writer> HelpTemplate<'cmd, 'writer> {
    /// Writes help for all arguments (options, flags, args, subcommands)
    /// including titles of a Parser Object to the wrapped stream.
    pub(crate) fn write_all_args(&mut self) {
        debug!("HelpTemplate::write_all_args");
        use std::fmt::Write as _;
        let header = &self.styles.get_header();

        let pos = self
            .cmd
            .get_positionals()
            .filter(|a| a.get_help_heading().is_none())
            .filter(|arg| should_show_arg(self.use_long, arg))
            .collect::<Vec<_>>();
        let non_pos = self
            .cmd
            .get_non_positionals()
            .filter(|a| a.get_help_heading().is_none())
            .filter(|arg| should_show_arg(self.use_long, arg))
            .collect::<Vec<_>>();
        let subcmds = self.cmd.has_visible_subcommands();

        let custom_headings = self
            .cmd
            .get_arguments()
            .filter_map(|arg| arg.get_help_heading())
            .collect::<FlatSet<_>>();

        let mut first = true;

        if subcmds {
            if !first {
                self.writer.push_str("\n\n");
            }
            first = false;
            let default_help_heading = Str::from("Commands");
            let help_heading = self
                .cmd
                .get_subcommand_help_heading()
                .unwrap_or(&default_help_heading);
            let _ = write!(
                self.writer,
                "{}{help_heading}:{}\n",
                header.render(),
                header.render_reset()
            );

            self.write_subcommands(self.cmd);
        }

        if !pos.is_empty() {
            if !first {
                self.writer.push_str("\n\n");
            }
            first = false;
            // Write positional args if any
            let help_heading = "Arguments";
            let _ = write!(
                self.writer,
                "{}{help_heading}:{}",
                header.render(),
                header.render_reset()
            );
            self.write_args(&pos, "Arguments", positional_sort_key);
        }

        if !non_pos.is_empty() {
            if !first {
                self.writer.push_str("\n\n");
            }
            first = false;
            let help_heading = "Options";
            let _ = write!(
                self.writer,
                "{}{help_heading}:{}",
                header.render(),
                header.render_reset()
            );
            self.write_args(&non_pos, "Options", option_sort_key);
        }
        if !custom_headings.is_empty() {
            for heading in custom_headings {
                let args = self
                    .cmd
                    .get_arguments()
                    .filter(|a| {
                        if let Some(help_heading) = a.get_help_heading() {
                            return help_heading == heading;
                        }
                        false
                    })
                    .filter(|arg| should_show_arg(self.use_long, arg))
                    .collect::<Vec<_>>();

                if !args.is_empty() {
                    if !first {
                        self.writer.push_str("\n\n");
                    }
                    first = false;
                    let _ = write!(
                        self.writer,
                        "{}{heading}:{}",
                        header.render(),
                        header.render_reset()
                    );
                    self.write_args(&args, heading, option_sort_key);
                }
            }
        }
    }
    /// Sorts arguments by length and display order and write their help to the wrapped stream.
    fn write_args(&mut self, args: &[&Arg], _category: &str, sort_key: ArgSortKey) {
        debug!("HelpTemplate::write_args {}", _category);
        use std::fmt::Write as _;
        let mut ord_v = Vec::new();
        let mut next_line_help = self.next_line_help || self.use_long;

        for &arg in args.iter().filter(|arg| {
            // If it's NextLineHelp we don't care to compute how long it is because it may be
            // NextLineHelp on purpose simply *because* it's so long and would throw off all other
            // args alignment
            should_show_arg(self.use_long, arg)
        }) {
            let key = sort_key(arg);
            let spec = self.render_arg_spec(arg);
            let help = self.render_arg_help(arg);
            ord_v.push((key, spec, help));
            next_line_help |= arg.is_next_line_help_set();
        }
        ord_v.sort_by(|left, right| {
            // Formatting key like this to ensure that:
            // 1. Argument has long flags are printed just after short flags.
            // 2. For two args both have short flags like `-c` and `-C`, the
            //    `-C` arg is printed just after the `-c` arg
            // 3. For args without short or long flag, print them at last(sorted
            //    by arg name).
            // Example order: -a, -b, -B, -s, --select-file, --select-folder, -x
            fn normalize(c: u8) -> u8 {
                if c == b' ' {
                    b'{'
                } else {
                    c.to_ascii_lowercase()
                }
            }
            let mut cmp = left.0.cmp(&right.0);
            if cmp == std::cmp::Ordering::Equal {
                let left_chars = left.1.as_styled_str().bytes().map(normalize);
                let right_chars = right.1.as_styled_str().bytes().map(normalize);
                cmp = left_chars.cmp(right_chars);
            }
            cmp
        });

        let mut longest = 0;
        for (_, spec, _) in &ord_v {
            longest = longest.max(spec.display_width());
        }
        let help_start = TAB_WIDTH + longest + TAB_WIDTH;
        let remaining = self.term_w - help_start;
        for (_, _, help) in &ord_v {
            let help_width = help.display_width();
            next_line_help |= self.term_w >= help_start
                && (help_start as f32 / self.term_w as f32) > 0.40
                && remaining < help_width;
        }

        if next_line_help {
            for (_, spec, mut help) in ord_v {
                help.indent("", NEXT_LINE_INDENT);
                self.writer.push_str("\n");
                if self.use_long {
                    self.writer.push_str("\n");
                }
                let _ = write!(self.writer, "{TAB}{spec}");
                let _ = write!(self.writer, "{NEXT_LINE_INDENT}{help}");
            }
        } else {
            let help_indent = format!("{:help_start$}", "");
            for (_, spec, mut help) in ord_v {
                help.indent("", &help_indent);
                let padding = longest - spec.display_width();
                self.writer.push_str("\n");
                let _ = write!(self.writer, "{TAB}{spec}{:padding$}{TAB}{help}", "");
            }
        }
    }

    fn render_arg_spec(&self, arg: &Arg) -> StyledStr {
        use std::fmt::Write as _;
        let literal = &self.styles.get_literal();
        let mut styled = StyledStr::new();

        if let Some(s) = arg.get_short() {
            let _ = write!(styled, "{}-{s}{}", literal.render(), literal.render_reset());
        } else if arg.get_long().is_some() {
            styled.push_str("    ");
        }

        if let Some(long) = arg.get_long() {
            if arg.get_short().is_some() {
                styled.push_str(", ");
            }
            let _ = write!(
                styled,
                "{}--{long}{}",
                literal.render(),
                literal.render_reset()
            );
        }

        styled.push_styled(&arg.stylize_arg_suffix(self.styles, None));

        styled
    }

    fn render_arg_help(&self, arg: &Arg) -> StyledStr {
        use std::fmt::Write as _;
        let literal = &self.styles.get_literal();

        let about = if self.use_long {
            arg.get_long_help()
                .or_else(|| arg.get_help())
                .unwrap_or_default()
        } else {
            arg.get_help()
                .or_else(|| arg.get_long_help())
                .unwrap_or_default()
        };
        let mut spec_vals = StyledStr::new();
        self.spec_vals(&mut spec_vals, arg);

        // Is help on next line, if so then indent
        let mut styled = about.clone();
        styled.replace_newline_var();

        if !spec_vals.is_empty() {
            if !styled.is_empty() {
                let sep = if self.use_long { "\n\n" } else { " " };
                styled.push_str(sep);
            }
            styled.push_styled(&spec_vals);
        }

        let possible_vals = arg.get_possible_values();
        if !possible_vals.is_empty() && !arg.is_hide_possible_values_set() && self.use_long_pv(arg)
        {
            debug!(
                "HelpTemplate::help: Found possible vals...{:?}",
                possible_vals
            );
            let longest = possible_vals
                .iter()
                .filter(|f| !f.is_hide_set())
                .map(|f| display_width(f.get_name()))
                .max()
                .expect("Only called with possible value");

            if !styled.is_empty() {
                styled.push_str("\n\n");
            }
            styled.push_str("Possible values:");
            for pv in possible_vals.iter().filter(|pv| !pv.is_hide_set()) {
                let indent = TAB;
                let name = pv.get_name();
                let _ = write!(
                    styled,
                    "\n{indent}- {}{name}{}",
                    literal.render(),
                    literal.render_reset()
                );
                if let Some(help) = pv.get_help() {
                    debug!("HelpTemplate::help: Possible Value help");
                    // To align help messages
                    let padding = longest - display_width(pv.get_name());
                    let _ = write!(styled, ": {:padding$}", "");

                    let mut help = help.clone();
                    help.replace_newline_var();
                    help.indent("", indent);
                    styled.push_styled(&help);
                }
            }
        }

        styled
    }

    /// Writes argument's help to the wrapped stream.
    fn help(
        &mut self,
        arg: Option<&Arg>,
        about: &StyledStr,
        spec_vals: &StyledStr,
        next_line_help: bool,
        longest: usize,
    ) {
        debug!("HelpTemplate::help");
        use std::fmt::Write as _;
        let literal = &self.styles.get_literal();

        // Is help on next line, if so then indent
        if next_line_help {
            debug!("HelpTemplate::help: Next Line...{:?}", next_line_help);
            self.writer.push_str("\n");
            self.writer.push_str(TAB);
            self.writer.push_str(NEXT_LINE_INDENT);
        }

        let spaces = if next_line_help {
            TAB.len() + NEXT_LINE_INDENT.len()
        } else if let Some(true) = arg.map(|a| a.is_positional()) {
            longest + TAB_WIDTH * 2
        } else {
            longest + TAB_WIDTH * 2 + 4 // See `fn short` for the 4
        };
        let trailing_indent = spaces; // Don't indent any further than the first line is indented
        let trailing_indent = self.get_spaces(trailing_indent);

        let mut help = about.clone();
        help.replace_newline_var();
        if !spec_vals.is_empty() {
            if !help.is_empty() {
                let sep = if self.use_long && arg.is_some() {
                    "\n\n"
                } else {
                    " "
                };
                help.push_str(sep);
            }
            help.push_styled(spec_vals);
        }
        let avail_chars = self.term_w.saturating_sub(spaces);
        debug!(
            "HelpTemplate::help: help_width={}, spaces={}, avail={}",
            spaces,
            help.display_width(),
            avail_chars
        );
        help.wrap(avail_chars);
        help.indent("", &trailing_indent);
        let help_is_empty = help.is_empty();
        self.writer.push_styled(&help);
        if let Some(arg) = arg {
            let possible_vals = arg.get_possible_values();
            if !possible_vals.is_empty()
                && !arg.is_hide_possible_values_set()
                && self.use_long_pv(arg)
            {
                debug!(
                    "HelpTemplate::help: Found possible vals...{:?}",
                    possible_vals
                );
                let longest = possible_vals
                    .iter()
                    .filter(|f| !f.is_hide_set())
                    .map(|f| display_width(f.get_name()))
                    .max()
                    .expect("Only called with possible value");

                let spaces = spaces + TAB_WIDTH - TAB_WIDTH;
                let trailing_indent = spaces + TAB_WIDTH;
                let trailing_indent = self.get_spaces(trailing_indent);

                if !help_is_empty {
                    let _ = write!(self.writer, "\n\n{:spaces$}", "");
                }
                self.writer.push_str("Possible values:");
                for pv in possible_vals.iter().filter(|pv| !pv.is_hide_set()) {
                    let name = pv.get_name();
                    let _ = write!(
                        self.writer,
                        "\n{:spaces$}{TAB}- {}{name}{}",
                        "",
                        literal.render(),
                        literal.render_reset()
                    );
                    if let Some(help) = pv.get_help() {
                        debug!("HelpTemplate::help: Possible Value help");

                        // To align help messages
                        let padding = longest - display_width(pv.get_name());
                        let _ = write!(self.writer, ": {:padding$}", "");

                        let avail_chars = if self.term_w > trailing_indent.len() {
                            self.term_w - trailing_indent.len()
                        } else {
                            usize::MAX
                        };

                        let mut help = help.clone();
                        help.replace_newline_var();
                        help.wrap(avail_chars);
                        help.indent("", &trailing_indent);
                        self.writer.push_styled(&help);
                    }
                }
            }
        }
    }

    fn spec_vals(&self, styled: &mut StyledStr, a: &Arg) {
        debug!("HelpTemplate::spec_vals: a={}", a);
        let mut first = true;

        #[cfg(feature = "env")]
        if let Some(ref env) = a.env {
            if !a.is_hide_env_set() {
                debug!(
                    "HelpTemplate::spec_vals: Found environment variable...[{:?}:{:?}]",
                    env.0, env.1
                );
                let mut val = env.0.to_string_lossy().into_owned();
                if !a.is_hide_env_values_set() {
                    use std::fmt::Write as _;
                    let _ = write!(
                        val,
                        "={}",
                        env.1
                            .as_ref()
                            .map(|s| s.to_string_lossy())
                            .unwrap_or_default()
                    );
                }
                let mut iter = [Cow::from(val)].into_iter();
                self.write_spec(styled, &mut first, "env", &mut iter);
            }
        }

        if a.is_takes_value_set() && !a.is_hide_default_value_set() && !a.default_vals.is_empty() {
            debug!(
                "HelpTemplate::spec_vals: Found default value...[{:?}]",
                a.default_vals
            );
            let mut iter = a.default_vals.iter().map(|val| val.to_string_lossy());
            self.write_spec(styled, &mut first, "default", &mut iter);
        }

        let als = a
            .aliases
            .iter()
            .filter(|&als| als.1) // visible
            .map(|als| Cow::from(als.0.as_str())) // name
            .collect::<Vec<_>>();
        if !als.is_empty() {
            debug!("HelpTemplate::spec_vals: Found aliases...{:?}", aaliases);
            let mut iter = als.into_iter();
            self.write_spec(styled, &mut first, "aliases", &mut iter);
        }

        let als = a
            .short_aliases
            .iter()
            .filter(|&als| als.1) // visible
            .map(|&als| Cow::from(als.0.to_string())) // name
            .collect::<Vec<_>>();
        if !als.is_empty() {
            debug!(
                "HelpTemplate::spec_vals: Found short aliases...{:?}",
                a.short_aliases
            );
            let mut iter = als.into_iter();
            self.write_spec(styled, &mut first, "short aliases", &mut iter);
        }

        let possible_vals = a.get_possible_values();
        if !possible_vals.is_empty() && !a.is_hide_possible_values_set() && !self.use_long_pv(a) {
            debug!(
                "HelpTemplate::spec_vals: Found possible vals...{:?}",
                possible_vals
            );

            let mut iter = possible_vals
                .iter()
                .filter(|pv| !pv.is_hide_set())
                .map(|pv| Cow::from(pv.get_name()));
            self.write_spec(styled, &mut first, "possible values", &mut iter);
        }
    }

    fn write_spec(
        &self,
        styled: &mut StyledStr,
        first: &mut bool,
        name: &str,
        values: &mut dyn Iterator<Item = Cow<str>>,
    ) {
        use std::fmt::Write as _;

        if !*first {
            let connector = if self.use_long { "\n" } else { " " };
            styled.push_str(connector);
        }
        *first = false;
        let _ = write!(styled, "[{name}: ");
        for (i, mut val) in values.enumerate() {
            if val.contains(char::is_whitespace) {
                val = Cow::from(format!("{val:?}"));
            }
            if 0 < i {
                styled.push_str(", ");
            }
            let _ = write!(styled, "{val}");
        }
        let _ = write!(styled, "]");
    }

    fn get_spaces(&self, n: usize) -> String {
        " ".repeat(n)
    }

    fn write_padding(&mut self, amount: usize) {
        use std::fmt::Write as _;
        let _ = write!(self.writer, "{:amount$}", "");
    }

    fn use_long_pv(&self, arg: &Arg) -> bool {
        self.use_long
            && arg
                .get_possible_values()
                .iter()
                .any(PossibleValue::should_show_help)
    }
}

/// Subcommand handling
impl<'cmd, 'writer> HelpTemplate<'cmd, 'writer> {
    /// Writes help for subcommands of a Parser Object to the wrapped stream.
    fn write_subcommands(&mut self, cmd: &Command) {
        debug!("HelpTemplate::write_subcommands");
        use std::fmt::Write as _;
        let literal = &self.styles.get_literal();

        // The shortest an arg can legally be is 2 (i.e. '-x')
        let mut longest = 2;
        let mut ord_v = Vec::new();
        for subcommand in cmd
            .get_subcommands()
            .filter(|subcommand| should_show_subcommand(subcommand))
        {
            let mut styled = StyledStr::new();
            let name = subcommand.get_name();
            let _ = write!(
                styled,
                "{}{name}{}",
                literal.render(),
                literal.render_reset()
            );
            if let Some(short) = subcommand.get_short_flag() {
                let _ = write!(
                    styled,
                    ", {}-{short}{}",
                    literal.render(),
                    literal.render_reset()
                );
            }
            if let Some(long) = subcommand.get_long_flag() {
                let _ = write!(
                    styled,
                    ", {}--{long}{}",
                    literal.render(),
                    literal.render_reset()
                );
            }
            longest = longest.max(styled.display_width());
            ord_v.push((subcommand.get_display_order(), styled, subcommand));
        }
        ord_v.sort_by(|a, b| (a.0, &a.1).cmp(&(b.0, &b.1)));

        debug!("HelpTemplate::write_subcommands longest = {}", longest);

        let next_line_help = self.will_subcommands_wrap(cmd.get_subcommands(), longest);

        for (i, (_, sc_str, sc)) in ord_v.into_iter().enumerate() {
            if 0 < i {
                self.writer.push_str("\n");
            }
            self.write_subcommand(sc_str, sc, next_line_help, longest);
        }
    }

    /// Will use next line help on writing subcommands.
    fn will_subcommands_wrap<'a>(
        &self,
        subcommands: impl IntoIterator<Item = &'a Command>,
        longest: usize,
    ) -> bool {
        subcommands
            .into_iter()
            .filter(|&subcommand| should_show_subcommand(subcommand))
            .any(|cmd| {
                let mut spec_vals = StyledStr::new();
                self.sc_spec_vals(&mut spec_vals, cmd);
                self.subcommand_next_line_help(cmd, &spec_vals, longest)
            })
    }

    fn write_subcommand(
        &mut self,
        sc_str: StyledStr,
        cmd: &Command,
        next_line_help: bool,
        longest: usize,
    ) {
        debug!("HelpTemplate::write_subcommand");

        let mut spec_vals = StyledStr::new();
        self.sc_spec_vals(&mut spec_vals, cmd);

        let about = cmd
            .get_about()
            .or_else(|| cmd.get_long_about())
            .unwrap_or_default();

        self.subcmd(sc_str, next_line_help, longest);
        self.help(None, about, &spec_vals, next_line_help, longest)
    }

    fn sc_spec_vals(&self, styled: &mut StyledStr, a: &Command) {
        debug!("HelpTemplate::sc_spec_vals: a={}", a.get_name());

        let mut first = true;

        let mut als = a
            .get_visible_short_flag_aliases()
            .map(|a| Cow::from(format!("-{a}")))
            .collect::<Vec<_>>();
        als.extend(a.get_visible_aliases().map(|s| Cow::from(s)));
        if !als.is_empty() {
            let mut iter = als.into_iter();
            self.write_spec(styled, &mut first, "aliases", &mut iter);
        }
    }

    fn subcommand_next_line_help(
        &self,
        cmd: &Command,
        spec_vals: &StyledStr,
        longest: usize,
    ) -> bool {
        if self.next_line_help | self.use_long {
            // setting_next_line
            true
        } else {
            // force_next_line
            let h = cmd.get_about().unwrap_or_default();
            let h_w = h.display_width() + spec_vals.display_width();
            let taken = longest + TAB_WIDTH * 2;
            self.term_w >= taken
                && (taken as f32 / self.term_w as f32) > 0.40
                && h_w > (self.term_w - taken)
        }
    }

    /// Writes subcommand to the wrapped stream.
    fn subcmd(&mut self, sc_str: StyledStr, next_line_help: bool, longest: usize) {
        self.writer.push_str(TAB);
        self.writer.push_styled(&sc_str);
        if !next_line_help {
            let width = sc_str.display_width();
            let padding = longest + TAB_WIDTH - width;
            self.write_padding(padding);
        }
    }
}

const NEXT_LINE_INDENT: &str = "        ";

type ArgSortKey = fn(arg: &Arg) -> usize;

fn positional_sort_key(arg: &Arg) -> usize {
    arg.get_index().unwrap_or(0)
}

fn option_sort_key(arg: &Arg) -> usize {
    arg.get_display_order()
}

pub(crate) fn dimensions() -> (Option<usize>, Option<usize>) {
    #[cfg(not(feature = "wrap_help"))]
    return (None, None);

    #[cfg(feature = "wrap_help")]
    terminal_size::terminal_size()
        .map(|(w, h)| (Some(w.0.into()), Some(h.0.into())))
        .unwrap_or_else(|| (parse_env("COLUMNS"), parse_env("LINES")))
}

#[cfg(feature = "wrap_help")]
fn parse_env(var: &str) -> Option<usize> {
    some!(some!(std::env::var_os(var)).to_str())
        .parse::<usize>()
        .ok()
}

fn should_show_arg(use_long: bool, arg: &Arg) -> bool {
    debug!(
        "should_show_arg: use_long={:?}, arg={}",
        use_long,
        arg.get_id()
    );
    if arg.is_hide_set() {
        return false;
    }
    (!arg.is_hide_long_help_set() && use_long)
        || (!arg.is_hide_short_help_set() && !use_long)
        || arg.is_next_line_help_set()
}

fn should_show_subcommand(subcommand: &Command) -> bool {
    !subcommand.is_hide_set()
}

#[cfg(test)]
mod test {
    #[test]
    #[cfg(feature = "wrap_help")]
    fn wrap_help_last_word() {
        use super::*;

        let help = String::from("foo bar baz");
        assert_eq!(wrap(&help, 5), "foo\nbar\nbaz");
    }

    #[test]
    #[cfg(feature = "unicode")]
    fn display_width_handles_non_ascii() {
        use super::*;

        // Popular Danish tongue-twister, the name of a fruit dessert.
        let text = "rødgrød med fløde";
        assert_eq!(display_width(text), 17);
        // Note that the string width is smaller than the string
        // length. This is due to the precomposed non-ASCII letters:
        assert_eq!(text.len(), 20);
    }

    #[test]
    #[cfg(feature = "unicode")]
    fn display_width_handles_emojis() {
        use super::*;

        let text = "😂";
        // There is a single `char`...
        assert_eq!(text.chars().count(), 1);
        // but it is double-width:
        assert_eq!(display_width(text), 2);
        // This is much less than the byte length:
        assert_eq!(text.len(), 4);
    }
}
