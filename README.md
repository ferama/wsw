#

```shell
sc.exe create wsw start= auto DisplayName= WSW binpath= "C:\Users\Administrator\wsw.exe C:\Users\Administrator\ltest.exe C:\Users\Administrator\ltest.log"

sc.exe start wsw

sc.exe query wsw

sc.exe stop wsw; sc.exe delete wsw
```