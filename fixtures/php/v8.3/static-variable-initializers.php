<?php

function counter()
{
    static $start = 10 * 2 + 1;
    static $now = compute();
    return $start;
}
