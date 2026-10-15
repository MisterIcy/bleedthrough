<?php

function each_item(iterable $items): iterable
{
    foreach ($items as $item) {
        yield $item;
    }
}
