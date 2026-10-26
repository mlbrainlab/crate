//! The password prompt: for an encrypted file list (to open the archive) or
//! for encrypted entries (before extracting or opening them).

use crate::i18n::{tr, trn};
use crate_core::{Archive, CancelToken};

use super::{PasswordFor, View, post, sentence};

impl View {
    pub(super) fn ask_password(&self, purpose: PasswordFor, error: &str) {
        self.state.borrow_mut().password_for = Some(purpose);
        self.g(|g| {
            g.set_password_error(error.into());
            g.set_password_checking(false);
            g.set_show_password(true);
        });
    }

    pub(super) fn wrong_password(&self, purpose: PasswordFor) {
        let attempts = {
            let mut s = self.state.borrow_mut();
            s.wrong_passwords += 1;
            s.wrong_passwords
        };
        let error = if attempts == 1 {
            tr("Incorrect password")
        } else {
            trn(
                "Incorrect password · {n} attempt",
                "Incorrect password · {n} attempts",
                attempts as u64,
            )
        };
        self.ask_password(purpose, &error);
    }

    pub(super) fn unlock(&self, password: String) {
        let purpose = self.state.borrow().password_for;
        self.g(|g| {
            g.set_password_checking(true);
            g.set_password_error("".into());
        });
        match purpose {
            Some(PasswordFor::Open) => {
                self.g(|g| g.set_status("loading".into()));
                self.open(Some(password));
            }
            Some(PasswordFor::Extract) => {
                let Some(mut archive) = self.state.borrow_mut().archive.take() else {
                    return;
                };
                self.state.borrow_mut().trying_password = Some(password.clone());
                std::thread::spawn(move || {
                    let ok = archive.check_password(&password, &CancelToken::new());
                    post(move |v| v.password_checked(archive, ok));
                });
            }
            None => self.g(|g| g.set_show_password(false)),
        }
    }

    fn password_checked(&self, archive: Archive, ok: crate_core::Result<bool>) {
        self.state.borrow_mut().archive = Some(archive);
        self.g(|g| g.set_password_checking(false));
        match ok {
            Ok(true) => {
                self.g(|g| g.set_show_password(false));
                let (open, request, edit) = {
                    let mut s = self.state.borrow_mut();
                    s.password_for = None;
                    s.password = s.trying_password.take();
                    (
                        s.pending_open.take(),
                        s.pending_request.take(),
                        s.pending_edit.take(),
                    )
                };
                match (open, request, edit) {
                    (Some(path), _, _) => self.run_open(path),
                    (None, Some(request), _) => self.run(request),
                    (None, None, Some((changes, backup))) => self.run_edit(changes, backup),
                    (None, None, None) => self.extract(),
                }
            }
            Ok(false) => self.wrong_password(PasswordFor::Extract),
            Err(e) => {
                self.g(|g| g.set_show_password(false));
                self.show_error(
                    &tr("Couldn’t check the password"),
                    &sentence(&e.to_string()),
                );
            }
        }
    }

    pub(super) fn cancel_password(&self) {
        {
            let mut s = self.state.borrow_mut();
            s.password_for = None;
            s.pending_open = None;
            s.pending_request = None;
            s.pending_edit = None;
        }
        self.g(|g| g.set_show_password(false));
    }
}
