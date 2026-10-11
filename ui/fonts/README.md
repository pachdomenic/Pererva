`RobotoFlex.ttf` — [Roboto Flex](https://github.com/googlefonts/roboto-flex),
ліцензія SIL Open Font License 1.1 (див. `OFL.txt`).

Шрифт зменшено з ~1.7 МБ до ~110 КБ: залишено лише вісь ваги `wght`,
решту осей зафіксовано на типових значеннях, набір символів обмежено
латиницею, кирилицею та типографськими знаками:

```sh
fonttools varLib.instancer RobotoFlex[...].ttf \
    opsz=drop wdth=drop GRAD=drop slnt=drop XOPQ=drop XTRA=drop \
    YOPQ=drop YTAS=drop YTDE=drop YTFI=drop YTLC=drop YTUC=drop \
    -o flex-wght.ttf
pyftsubset flex-wght.ttf --layout-features='*' --output-file=RobotoFlex.ttf \
    --unicodes="U+0000-00FF,U+0100-017F,U+0400-04FF,U+2000-206F,U+20AC,U+20B4,U+2116,U+2122,U+2190-21FF,U+2212,U+2022"
```
