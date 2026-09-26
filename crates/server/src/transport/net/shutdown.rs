//! 接入层的关闭信号与取消路径（`design.md` D11 的关闭序列）。
//!
//! 组合根持有 [`ShutdownHandle`] 并把它交给 [`crate::transport::net::NetListener::serve`]；信号触发后
//! listener 停止 accept、排空在途连接（宽限由 `daemon.shutdown_grace_ms` 决定），并把同一信号交给每个
//! 已升级的连接（[`crate::transport::net::PeerInfo::shutdown`]），使上层会话也能选择自己的取消路径。
//! 所有异步任务因此都有所有者与取消路径，不遗留 detached task（`AGENTS.md` §7）。

use tokio::sync::watch;

/// 关闭信号（可克隆）：所有克隆观察同一个触发。
#[derive(Debug, Clone)]
pub struct Shutdown {
    receiver: watch::Receiver<bool>,
}

/// 触发关闭的句柄；只有组合根持有并负责在关闭序列开始时触发一次。
#[derive(Debug)]
pub struct ShutdownHandle {
    sender: watch::Sender<bool>,
}

impl Shutdown {
    /// 建立一对关闭信号与句柄。
    pub fn channel() -> (ShutdownHandle, Shutdown) {
        let (sender, receiver) = watch::channel(false);
        (ShutdownHandle { sender }, Shutdown { receiver })
    }

    /// 是否已请求关闭（不阻塞）。
    pub fn is_shutdown(&self) -> bool {
        *self.receiver.borrow()
    }

    /// 等待关闭被请求（已请求时立即返回）。
    ///
    /// 消耗 `self`：信号常需要被移进 `'static` 的 serve/会话任务里，借用形式会强制多绕一层克隆。
    pub async fn wait(self) {
        let mut receiver = self.receiver;
        loop {
            if *receiver.borrow_and_update() {
                return;
            }
            // 句柄被提前丢弃（例如测试里直接 drop）：按已关闭处理，不由等待方持有运行期资源。
            if receiver.changed().await.is_err() {
                return;
            }
        }
    }
}

impl ShutdownHandle {
    /// 请求关闭。可重复调用；没有等待方时也不报错。
    pub fn trigger(&self) {
        let _ = self.sender.send(true);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn wait_returns_immediately_after_trigger() {
        let (handle, shutdown) = Shutdown::channel();
        assert!(!shutdown.is_shutdown());
        handle.trigger();
        assert!(shutdown.is_shutdown());
        // 触发后等待必须立即返回（否则关闭序列会挂住）。
        tokio::time::timeout(std::time::Duration::from_secs(1), shutdown.wait())
            .await
            .expect("关闭信号已触发时 wait 必须立即返回");
    }

    #[tokio::test]
    async fn wait_resolves_when_the_handle_is_dropped() {
        let (handle, shutdown) = Shutdown::channel();
        drop(handle);
        tokio::time::timeout(std::time::Duration::from_secs(1), shutdown.wait())
            .await
            .expect("句柄被丢弃时等待方必须结束，而不是永久挂起");
    }

    #[tokio::test]
    async fn clone_observes_the_same_trigger() {
        let (handle, shutdown) = Shutdown::channel();
        let clone = shutdown.clone();
        assert!(!clone.is_shutdown());
        let waiting = tokio::spawn(async move { clone.wait().await });
        handle.trigger();
        tokio::time::timeout(std::time::Duration::from_secs(1), waiting)
            .await
            .expect("等待任务必须结束")
            .expect("等待任务不得 panic");
    }
}
