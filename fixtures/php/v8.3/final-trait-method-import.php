<?php

class C
{
    use T {
        foo as final;
        bar as protected baz;
    }
}
