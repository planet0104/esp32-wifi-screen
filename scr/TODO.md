1. [OK.] html删除配置功能(wifi、屏幕配置)
2. [OK.] 画布功能(文本/Gif/形状/线条)，尝试imageproc库
3. [OK.] HTML 画布例子源码
4. [OK.] HTML设置/WIFI连接中等，相关的屏幕提示(基于画布功能)
   [OK.] html测试示例中的时钟无法运行

5. [OK.] 基于屏幕截图的wifi屏幕驱动程序(托盘)
   [OK.] 支持使用websocket+lz4发送并设置时间间隔(ms)
   [OK.] 显示鼠标指针
   [OK.] 使用 fast resize

https://github.com/VirtualDisplay/Virtual-Display-Driver

6. [OK.] USBScreen编辑器适配wifi屏幕
7. [OK.] WiFi断开后，自动重连功能(如果配置了wifi)
8. [OK.] WiFi断开后，如果用户打开了设置界面，取消自动重启
9. [OK.] WiFi网关变了(例如小米手机热点，网关ip随机)，原有IP无法使用！
10. [OK.] 内存不足问题
11. [OK.] 删除mqtt配置功能
12. [OK.] Websocket传输画布绘制测试 

13. [OK.] 远程mqtt服务器功能
14. [OK.] mqtt测试发送预存图片
15. [OK.] USB Screen编辑器的进度条bug缩放问题修复
16. [OK.] 完善html页面的屏幕设置，调试各个屏幕
    初测通过的屏幕:
    【OK.】ST7796 320x480
    【OK.】ST7735S 128x160
    【OK.】ST7735S 80x160
    【OK.】ST7789 240x240
    【OK.】ST7789 240x320
    【OK.】ST7789V 135x240
17. [OK.] 最终测试gif/jpg/lz4解码能否成功
18. [OK.] 代码整理、上传新的github地址
19. [OK.] 注意修改 录屏器以及USB Screen中的display config，宽高取旋转后的宽高。
20. [ x ] 打包分为带日志版本，和不带日志版本。
21. [ x ] WiFi屏幕的长时间稳定性测试