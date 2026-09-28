# 安全设置（管理员）

## 用途

管理**你自己**这个管理员账号的安全：修改密码、两步验证。

## 入口

**系统设置 → 安全设置**。

![管理员安全设置](/screenshots/admin/security.png)

## 修改密码

输入旧密码、新密码、确认新密码。修改后你在其他设备上的登录会失效。

## 两步验证

强烈建议所有管理员都开启：

1. 点 **开启两步验证**，用验证器 App 扫码；
2. 输入 App 里的 6 位数字；
3. 保存弹出的**恢复码**（可以复制或下载 txt）。每个恢复码只能用一次；
4. 页面显示“已开启”和剩余恢复码数量。

之后登录都需要动态码。**重新生成恢复码** 和 **关闭两步验证** 都需要输入当前的动态码或恢复码。

## 忘记密码或丢了手机

- 其他超级管理员可以在 **权限管理** 里重置你的两步验证；
- 只有一个超级管理员时，在服务器上执行：

```bash
zebra-store --config /opt/zebra/config.yml admin reset-2fa --username admin
zebra-store --config /opt/zebra/config.yml admin reset-password --username admin
```

重置后该管理员的所有登录都会失效。旧写法 `admin reset2fa`（不带连字符）同样可用。
Docker 部署在命令前加 `docker compose exec zebra`，配置路径用 `/app/config.yml`。

## 登录限流

后台登录只统计**失败**的尝试，并且按“用户名 + IP”分别计数：同一个 IP（例如公司出口）下的多位管理员互不影响，
成功登录会清零该账号的计数。连续失败达到 `security.login_rate_limit.max_attempts` 次后，
该账号在这个 IP 上被锁 `block_seconds` 秒；两步验证码输错同样按失败计数。
