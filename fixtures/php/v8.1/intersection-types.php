<?php

function f(Countable&Traversable $x): Iterator&Countable
{
    return $x;
}
