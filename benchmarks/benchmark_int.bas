10 REM NextBASIC benchmark with integer variables (%a..%z): loop and arith
20 RUN AT 3: LAYER 0: CLS: DIM t(2): LET k=0
30 PRINT "NextBASIC integer benchmark, 28 MHz": PRINT "running..."
100 GO SUB 9000: LET %i=0: REPEAT : LET %i=%i+1: REPEAT UNTIL %i=10000: GO SUB 9100
110 GO SUB 9000: LET %s=0: LET %i=0: REPEAT : LET %i=%i+1: LET %s=%s+i*7: LET %s=%s MOD 1000: REPEAT UNTIL %i=5000: GO SUB 9100
200 CLS: PRINT "NextBASIC integer benchmark, 28 MHz": PRINT
210 PRINT "loop";TAB 10;t(1);" ms": PRINT "arith";TAB 10;t(2);" ms"
220 PRINT: PRINT "arith ";%s
230 STOP
9000 LET t0=PEEK 23672+256*PEEK 23673+65536*PEEK 23674: RETURN
9100 LET k=k+1: LET t(k)=(PEEK 23672+256*PEEK 23673+65536*PEEK 23674-t0)*20: RETURN
